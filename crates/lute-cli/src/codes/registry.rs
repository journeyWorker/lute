use super::Code;

pub(crate) const CODES: &[Code] = &[
    Code {
        code: "E-ACCEPT-TARGET",
        summary: r#"An `::accept` directive names no quest, gives `quest` a value that is not a quoted quest id, gives `at` a value other than `"nextRun"`, or targets a quest declared `accept="external"` that already activates with its parent (making the acceptance a no-op)."#,
        spec: &["dsl 0.21.0 §7a.3", "dsl 0.24.0 §2", "dsl 0.25.0 §5"],
    },
    Code {
        code: "E-ADVANCE-CASCADE",
        summary: "A repeatable beat answers the clock raise that presents it and advances into the next position, creating a repeating nested `advances:` cascade.",
        spec: &["dsl 0.31.0 §1"],
    },
    Code {
        code: "E-AGE-GATE",
        summary: r#"An age-gated `<match subject="app.rating">` covers neither a `teen` arm nor an `<otherwise>`, so a release build could hit no matching case."#,
        spec: &["dsl §11.2"],
    },
    Code {
        code: "E-APP-READONLY",
        summary: "A `::set` directive writes to the `app.*` namespace, which content may never write because the engine or settings layer owns it exclusively.",
        spec: &["dsl §9.5"],
    },
    Code {
        code: "E-ARM-DEAD",
        summary: "A gated content line, `<match>` arm, `<choice when>`, or `::jump{when}` carries a `when` guard that is provably always false, so it can never be shown or taken.",
        spec: &["dsl 0.4.0 §5.2", "dsl 0.4.0 §7.2", "dsl 0.20.0 §5"],
    },
    Code {
        code: "E-AS-REMOVED",
        summary: "A choice uses the removed `as` attribute; `into` names the run fact a choice records.",
        spec: &["dsl 0.10.0 §4"],
    },
    Code {
        code: "E-ASSET-DECOMPOSE",
        summary: "An asset id string has the wrong number of `/`- or `.`-separated segments for its declared kind, or a fixed (`const`) segment does not match the value the kind requires.",
        spec: &["dsl §7.2"],
    },
    Code {
        code: "E-ASSET-SEGMENT",
        summary: "A segment of an asset id is not a member of its declared enum, is not a number where one is required, or is not a known id in its provider's catalog.",
        spec: &["dsl §7.2"],
    },
    Code {
        code: "E-ASSET-UNKNOWN-ID",
        summary: "A pure-query asset kind's id string is not a known id in its declared provider's catalog.",
        spec: &["dsl §7.2"],
    },
    Code {
        code: "E-AT-CONTEXT",
        summary: "A directive (plain `::directive` or `::use`) carries the timeline-position attribute `at` outside a `<track>` clip, where `at` is not valid.",
        spec: &["dsl §7.5"],
    },
    Code {
        code: "E-ATTR-DEF-DYNAMIC",
        summary: "A `@def` used as a directive attribute value, or as a `::use` argument landing in a component-body attribute, does not fold to a compile-time constant because it depends on state.",
        spec: &["dsl §5.1"],
    },
    Code {
        code: "E-ATTR-QUOTE",
        summary: r#"An attribute value is delimited with a single quote or a curly (word-processor) quote instead of the required straight double quote `"`."#,
        spec: &["dsl §4.4", "dsl §4.5"],
    },
    Code {
        code: "E-ATTR-TYPE",
        summary: "An attribute's value does not match its declared type — for example a non-numeric `duration`/`zoom`/`shake`, a non-boolean flag, a bare identifier where a quoted string is required, or a value naming no member of its provider, domain, or entity kind.",
        spec: &[],
    },
    Code {
        code: "E-AUTHOR-CASE",
        summary: "An authoring attribute, tag attribute or frontmatter key is not lowerCamelCase (it contains `_`); the message names its lowerCamelCase spelling.",
        spec: &["dsl 0.37.0 §2.1"],
    },
    Code {
        code: "E-BAD-ENUM",
        summary: "A value is not a member of the closed enum, domain, or entity kind it must belong to — for example a content line's `emotion=` outside the speaker's declared `emotions:`, or an entity id outside its kind.",
        spec: &["dsl 0.24.0 §4"],
    },
    Code {
        code: "E-BEAT-ATTR",
        summary: "A beat's `on`, `target`, `priority`, or `once` attribute is malformed — an `on` that is not a name, a `target` on an occasion not declared `target: true`, a non-integer `priority`, a `once` outside `run`/`user`/`false`, a beat key with no `on`, a `when` reading the scene's own not-yet-existing `scene.*` state, or a `spentBy` beside `once: false` or a `share` key.",
        spec: &["dsl 0.21.0 §3", "dsl 0.21.0 §5", "dsl 0.28.0 §6"],
    },
    Code {
        code: "E-BEAT-ID-DUP",
        summary: "Two declarations of one lore document share an id: a bundle `<beat>` id repeated, or an `<entry>` whose id is a `<beat>`'s — the beat's canonical id `<document id>.<id>` is also the entry's alias, so `visited()` or a play's `expect.winner` would name both.",
        spec: &["dsl 0.28.0 §7"],
    },
    Code {
        code: "E-BEAT-UNREACHABLE",
        summary: "A scene beat's `when` guard is provably always false, so the beat can never be selected.",
        spec: &["dsl 0.21.0 §5"],
    },
    Code {
        code: "E-BRANCH-ALL-GUARDED",
        summary: "A non-empty `<branch>` has every `<choice>` guarded by a `when`, so every guard could be false at once and present an empty menu; at least one unguarded choice is required.",
        spec: &["dsl §11.1"],
    },
    Code {
        code: "E-BRANCH-EMPTY",
        summary: "A `<branch>` or `<hub>` contains no `<choice>` at all, which would flatten to an unroutable choice.",
        spec: &["dsl §7.3"],
    },
    Code {
        code: "E-BRANCH-PROMPT",
        summary: "A `<branch prompt>` or `<hub prompt>` value is missing or an empty string, but the prompt must be the non-empty sentence the UI shows verbatim.",
        spec: &["dsl 0.11.1 §4", "dsl 0.23.0 §4"],
    },
    Code {
        code: "E-BRANCH-TIMEOUT",
        summary: "A `<branch timeout>` value does not parse as a positive whole number of seconds.",
        spec: &["dsl 0.11.1 §4"],
    },
    Code {
        code: "E-CAMERA-EMPTY",
        summary: "A `::camera` directive names none of `focus`, `framing`, `move` or `transition`, so it has nothing to do.",
        spec: &["dsl 0.37.0 §3.3"],
    },
    Code {
        code: "E-CAMERA-REMOVED",
        summary: "A `::camera` directive uses a removed numeric or legacy attribute (`zoom`, `moveX`, `moveY`, `shake`, `reset`, `easing`); camera values are now project-declared `framing`, `cameraMove` or `transition` members, so the attribute must be migrated by hand.",
        spec: &["dsl 0.37.0 §3.3", "dsl 0.37.0 §4"],
    },
    Code {
        code: "E-CAPABILITY-MISMATCH",
        summary: "Two documents in the same project resolve different capability snapshots, so the project has no single `capabilityVersion` to index.",
        spec: &["dsl §13"],
    },
    Code {
        code: "E-CAST-UNKNOWN",
        summary: "A content line's speaker, or a `::actor{character}`/`::camera{focus}` literal, names an id outside the project's declared cast.",
        spec: &["dsl 0.23.0 §7", "dsl 0.24.0 §4"],
    },
    Code {
        code: "E-CEL-PARSE",
        summary: "A CEL expression in a condition slot, def body, or `present:`/beat `when` does not parse as valid CEL syntax.",
        spec: &["dsl 0.4.0 §8.1"],
    },
    Code {
        code: "E-CEL-PROFILE",
        summary: "A CEL expression uses a construct outside the restricted Lute-CEL profile — a disallowed function call, a comprehension macro, a map/struct literal, a bare identifier that is not a state path or def, a reserved internal token, or `visited(…)` used outside a condition slot.",
        spec: &["dsl §8.4", "dsl 0.21.0 §7a.1"],
    },
    Code {
        code: "E-CEL-TYPE",
        summary: "A condition's types cannot mean what is written: a comparison between a bool, a number and a string (`visited('x') > 2`, `run.oil == true`, `run.day == 'monday'`), an ordering of anything but numbers (`run.hour >= 'h03'`), a non-bool operand of `&&` / `||` / `!` / `?:` or condition, arithmetic that cannot be computed, or an operand of the integer modulo operator `%` that is not an integer.",
        spec: &["dsl 0.24.0 §1", "dsl 0.28.0 §1"],
    },
    Code {
        code: "E-CG-LAYOUT",
        summary: "A `::cg` directive uses the removed `full` attribute; only a project-declared `cgLayout` member written as `layout=` can replace it, so it must be migrated by hand.",
        spec: &["dsl 0.37.0 §3.3", "dsl 0.37.0 §4"],
    },
    Code {
        code: "E-CHAPTERS",
        summary: "The project's `chapters:` is malformed — not a list of `{ on, scenes }` chains, a key that is neither (a chain names its occasion with `on:`, not `occasion:`), an entry that is no scene id, a scene listed twice, two chains on one occasion — or the manifest still uses the retired `sequence:` key; or a chain names an occasion no plugin declares (or, shape-only, a near-miss of one other beats answer), lists an id no scene declares (a bundle beat, lore entry or document is named as such), lists a scene whose own `on:` answers another occasion, or, on an occasion raised for a target, lists a scene with no `target:` (it would play for every target). A malformed chain is not applied; the other chains are. Reported at the manifest line — a missing `target:` at the scene's `id:` — and the documents are still checked.",
        spec: &["dsl 0.28.0 §4"],
    },
    Code {
        code: "E-CHECK-ENGINE-SEMANTICS",
        summary: "A construct requires a semantic capability the selected engine does not support.",
        spec: &["dsl 0.33.0 §4"],
    },
    Code {
        code: "E-CHOICE-DUP",
        summary: "A `<branch>` or `<hub>` declares two `<choice>` elements with the same `id`, but choice ids must be unique within their branch or hub.",
        spec: &["dsl §11.1"],
    },
    Code {
        code: "E-CHOICELOG-READ",
        summary: "A guard or condition reads a reserved choice-log path, which may not be read from a guard or condition.",
        spec: &["dsl §9.6"],
    },
    Code {
        code: "E-CLIP-OVERLAP",
        summary: "Two clips in the same `<track>` have overlapping `[at, at+duration)` intervals.",
        spec: &["dsl §11.4"],
    },
    Code {
        code: "E-CLIP-TIMING",
        summary: "A single `<track>` clip carries both `at` (an absolute timeline position) and `delay` (a relative nudge), which are mutually exclusive on one clip.",
        spec: &["dsl §7.5", "dsl §11.4"],
    },
    Code {
        code: "E-CLOCK-DECL",
        summary: "A `clock:` declaration is malformed (a missing field, a `last.slot` not in `slots`, `days: 0`, both `last` and `days`), or a project declares more than one `clock:`.",
        spec: &["dsl 0.24.0 §1"],
    },
    Code {
        code: "E-CLOCK-END",
        summary: "A `lute play`/`lute test` `advance:` step runs while a finite clock has ended, or starts past its last position — the clock already raised its last `dayEnd` and stopped, so play a `newRun` or end the script.",
        spec: &["dsl 0.27.0 §4"],
    },
    Code {
        code: "E-COMMENT-UNTERMINATED",
        summary: "A `/* … */` block comment ran to the end of the file without a closing `*/`.",
        spec: &["dsl §4.2"],
    },
    Code {
        code: "E-COMPILE-COMPONENT",
        summary: "A `::use` invocation reaches compilation still unresolved — used inside a `<timeline>` clip (not allowed), naming no resolvable component, or with args that do not match the component's params — a case the check gate should already have caught.",
        spec: &[],
    },
    Code {
        code: "E-COMPILE-EXPAND",
        summary: "A CEL slot or beat `when` fails to expand at compile time — an expansion cycle, an unknown def, or an arity mismatch.",
        spec: &[],
    },
    Code {
        code: "E-COMPILE-INTERNAL",
        summary: "The compiler reached a state it cannot recover from; this is a Lute bug, so please report it with the document that triggers it.",
        spec: &[],
    },
    Code {
        code: "E-COMPONENT-ARG",
        summary: "A `::use` invocation's arguments do not match the named component's declared params — an unknown argument, a missing required param, a value of the wrong type, an incompatible default, or (for a `speaker` param) an argument that is not a literal cast id.",
        spec: &["dsl §13", "dsl 0.24.0 §3", "dsl 0.26.0 §3.3"],
    },
    Code {
        code: "E-COMPONENT-BODY",
        summary: "A component body contains a construct that is not presentational — a `::set`, `<branch>`, `<hub>`, `<timeline>`, `<objective>`, `<on>`, `::assert`, or `::retract` that writes or affects state, or a `::use` of an effectful inner component without declaring `effects: true` itself.",
        spec: &["dsl 0.4.0 §6.2", "dsl 0.24.0 §4"],
    },
    Code {
        code: "E-COMPONENT-CYCLE",
        summary: "A chain of `::use` invocations expands into a cycle across components.",
        spec: &["dsl §13"],
    },
    Code {
        code: "E-COMPONENT-DUP",
        summary: "Two different component files declare the same `component:` name.",
        spec: &[],
    },
    Code {
        code: "E-COMPONENT-INSTANCE-DUPLICATE",
        summary: "The same component and instance key occur twice in one immediate expansion owner.",
        spec: &["dsl 0.36.0 §2.3"],
    },
    Code {
        code: "E-COMPONENT-INSTANCE-INVALID",
        summary: "`instance` is empty, exceeds 64 characters, uses a non-ASCII token, or appears more than once as an identity attribute on one `::use`.",
        spec: &["dsl 0.36.0 §2.3"],
    },
    Code {
        code: "E-COMPONENT-PARSE",
        summary: "A component file cannot be read, resolved, or parsed cleanly — an unresolvable `components:` import path, a missing `component:` name, or a malformed `params:` entry.",
        spec: &[],
    },
    Code {
        code: "E-COMPONENT-STATE",
        summary: "A component body reads or writes ambient state directly — a CEL reference to a state path, a fact query, or a directive that declares state/bridge-result writes — instead of binding it through a param.",
        spec: &["dsl 0.4.0 §6.1", "dsl 0.4.0 §6.2", "dsl 0.24.0 §4"],
    },
    Code {
        code: "E-COMPONENT-UNDECLARED",
        summary: "A `::use`'s `component` attribute names a component absent from the resolved `components:` table.",
        spec: &["dsl §13"],
    },
    Code {
        code: "E-CONN-CYCLE",
        summary: "The project's `after` prerequisite graph contains a cycle, so no evaluation order can satisfy every `after` clause simultaneously.",
        spec: &["dsl §4.1"],
    },
    Code {
        code: "E-CONN-EPISODE-ID-DUP",
        summary: "Two documents (or a document and a derived key) share the same document id in the project's shared id namespace.",
        spec: &["dsl 0.15.0 §2", "dsl 0.19.0 §2.1"],
    },
    Code {
        code: "E-CONN-FORMULA-TOO-COMPLEX",
        summary: "An `after` prerequisite formula's atom count exceeds the defensive cap, a pattern typical of a pathological or machine-generated formula rather than one an author types by hand.",
        spec: &["dsl §4.1"],
    },
    Code {
        code: "E-CONN-PROFILE",
        summary: "An `after` prerequisite formula uses a construct outside the restricted prerequisite profile — anything other than `visited(…)`/`completed(…)`/`active(…)` atoms combined with `&&`/`||` and parentheses.",
        spec: &["dsl §4.1"],
    },
    Code {
        code: "E-CONN-UNKNOWN-NODE",
        summary: "An `after` prerequisite formula's `visited(K)`/`completed(Q)`/`active(Q)` atom names a node that does not exist anywhere in the project.",
        spec: &["dsl §2.3", "dsl §4.1"],
    },
    Code {
        code: "E-CONN-UNREACHABLE",
        summary: "A scene, quest, or beat is provably unreachable — no evaluation order can ever satisfy its `after` prerequisite.",
        spec: &["dsl §4.1", "dsl §4.2"],
    },
    Code {
        code: "E-CONSTRAINT-DECL",
        summary: "A project constraint declaration is malformed: it has an unknown key or kind, misses a required field, names a bad node, or repeats an id.",
        spec: &["dsl 0.34.0 §5.1"],
    },
    Code {
        code: "E-CONSTRAINT-VIOLATED",
        summary: "A project constraint is violated; the diagnostic carries the verdict's evidence, bounded scope when applicable, and causal spans.",
        spec: &["dsl 0.34.0 §5.2"],
    },
    Code {
        code: "E-CONTENT-LINE-BRACKET",
        summary: "A content line's attributes are written with `[…]` instead of the required `{…}` (the same delimiter `::directive{…}` uses).",
        spec: &["dsl §2.1"],
    },
    Code {
        code: "E-CONTENT-OUTSIDE-SECTION",
        summary: "A content-shaped line (`@speaker…`, `::directive`, a `<tag>`) appears before the document's first `## ` section heading, since content only belongs inside a section body.",
        spec: &["dsl 0.5.0 §2.1", "dsl 0.6.0 §3.3"],
    },
    Code {
        code: "E-CONTEXT-POSITION",
        summary: "A context position query has an invalid coordinate or falls outside the source file.",
        spec: &["dsl 0.36.0 §3.3", "dsl 0.36.0 §8"],
    },
    Code {
        code: "E-CONTEXT-TARGET",
        summary: "A context target key is unknown or ambiguous in the project graph.",
        spec: &["dsl 0.36.0 §3.1", "dsl 0.36.0 §8"],
    },
    Code {
        code: "E-DATALOG-FUNCTION",
        summary: "An `::assert`/`::retract` payload, a `facts:` entry, or a `rules:` term uses a compound/function term such as `f(g(x))`, but facts and rule terms admit only ground identifiers/booleans.",
        spec: &["dsl 0.3.0 §7.1"],
    },
    Code {
        code: "E-DATALOG-GUARD-FACT",
        summary: "A rule-body CEL guard reads the fact store or narrative time via `holds`, `count`, `validAt`, or `now`, which a guard is not allowed to depend on.",
        spec: &["dsl 0.3.0 §7.2", "dsl 0.3.0 §7.3"],
    },
    Code {
        code: "E-DATALOG-PARSE",
        summary: "A `facts:`/`rules:` entry is not a quoted string, or a quoted `facts:`/`rules:` string fails to parse as a well-formed ground fact or rule.",
        spec: &["dsl 0.3.0 §4", "dsl 0.3.0 §5"],
    },
    Code {
        code: "E-DATALOG-UNSAFE",
        summary: "A rule's negated atom, `!=` comparison, or CEL guard reads a variable that no positive body atom binds first, so the rule cannot be safely evaluated.",
        spec: &["dsl 0.27.0 §3", "dsl 0.24.0 §3"],
    },
    Code {
        code: "E-DATALOG-UNSTRATIFIED",
        summary: "A rule's negation edge closes a cycle in the predicate-dependency graph (including `p :- not p`), so the ruleset cannot be stratified.",
        spec: &["dsl §7.2"],
    },
    Code {
        code: "E-DEF-DECL",
        summary: "A `defs:` entry is malformed — not a CEL string or a `{ type?, params?, cel }` mapping, an unknown key, a missing `cel:`, or a `type:` that is neither inferable from the body nor consistent with it.",
        spec: &["dsl 0.21.0 §7b"],
    },
    Code {
        code: "E-DEFAULTS-KEY",
        summary: "A manifest's `defaults:` block names a key outside the closed defaultable-frontmatter set, or gives a defaultable key a value of the wrong shape.",
        spec: &["dsl 0.10.0 §6.1"],
    },
    Code {
        code: "E-DELIVERY-CONFLICT",
        summary: "A content line sets more than one of the mutually exclusive delivery flags `mono`, `os`, `vo`.",
        spec: &["dsl 0.2.2 D7"],
    },
    Code {
        code: "E-DELIVERY-FLAG-VALUE",
        summary: "A delivery flag (`mono`/`os`/`vo`) is given a value, but a delivery flag is bare and takes none.",
        spec: &["dsl 0.2.2 D7"],
    },
    Code {
        code: "E-DELIVERY-NARRATOR",
        summary: "A `narrator` content line carries a delivery attribute, but narration takes no delivery.",
        spec: &["dsl 0.1.0 §12.1"],
    },
    Code {
        code: "E-DEPENDS-CYCLE",
        summary: "Two or more plugins' `depends` declarations form a cycle, so their activation order cannot be resolved.",
        spec: &[],
    },
    Code {
        code: "E-DEPENDS-UNRESOLVED",
        summary: "A plugin's `depends` names another plugin id that is not installed in the project.",
        spec: &[],
    },
    Code {
        code: "E-DEPENDS-VERSION",
        summary: "A plugin's `depends` names an installed plugin whose version does not satisfy the declared version range.",
        spec: &[],
    },
    Code {
        code: "E-DERIVE-TIER",
        summary: "A relation is declared `derive: true` but also declares a write `tier:`, though a derived relation has no write tier of its own.",
        spec: &["dsl 0.3.0 §4", "dsl 0.3.0 §7.1"],
    },
    Code {
        code: "E-DERIVE-UNDECLARED",
        summary: "A `rules:` entry's head names a relation that is not declared `derive: true` (a base relation, a reserved relation, or an entity-kind name), though only a derived relation may be a rule head.",
        spec: &["dsl 0.3.0 §7.1", "dsl 0.3.0 §3.1"],
    },
    Code {
        code: "E-DERIVED-WRITE",
        summary: "Content asserts or retracts a relation declared `derive: true`, though a derived relation is computed by `rules:` and must not be written directly.",
        spec: &["dsl 0.3.0 §5"],
    },
    Code {
        code: "E-DIFF-INPUT",
        summary: "A diff side cannot be read, safely materialized, or built into a complete project model.",
        spec: &["dsl 0.36.0 §4", "dsl 0.36.0 §8"],
    },
    Code {
        code: "E-DIFF-MODEL",
        summary: "A semantic diff cannot produce a complete project model for one side.",
        spec: &["dsl 0.36.0 §4", "dsl 0.36.0 §8"],
    },
    Code {
        code: "E-DOLLAR-OUTSIDE-MATCH",
        summary: "`$` (the match subject) is used outside a `<match>` block, where it is not valid.",
        spec: &["dsl §8.2"],
    },
    Code {
        code: "E-DOMAIN-DUP",
        summary: "A project's own `enums:`/`entities:` declaration (inline or reached through `uses:`/`extends:`) names a domain that a plugin or the core already declares, though a domain name must have exactly one source.",
        spec: &["dsl 0.3.0 §3", "dsl 0.9.0 D-D"],
    },
    Code {
        code: "E-DOMAIN-NAME-CLASH",
        summary: "One name is declared both as an `enums:` domain and as an entity kind — in one document or across the schemas a document merges. Enums and entity kinds share one domain namespace, so the kind's members would silently replace the enum's wherever the name types a value.",
        spec: &[],
    },
    Code {
        code: "E-DOMAIN-UNKNOWN",
        summary: "A content-line `emotion`/`action` slot, an entity attribute, or an implicit `anchor` read names a domain that no `enums:`/`entities:` declaration defines.",
        spec: &["dsl 0.9.0 D-C", "dsl 0.9.0 D-D"],
    },
    Code {
        code: "E-DUP-BRANCH",
        summary: "A `<branch id>` or `<hub id>` repeats an id already used by another branch or hub in the same episode, since branch and hub ids share one uniqueness domain.",
        spec: &["dsl §11.1", "dsl §7.3.2"],
    },
    Code {
        code: "E-DUP-LINE-CODE",
        summary: "Two content lines from the same speaker share the same `:line` `code=`, though a (speaker, code) pair must be unique — it is the voice-key/i18n identity join key.",
        spec: &["dsl §12"],
    },
    Code {
        code: "E-DUP-TRACK",
        summary: "Two `<track>`s in a `<timeline>` share the same track key.",
        spec: &[],
    },
    Code {
        code: "E-DUP-VOICEKEY",
        summary: "Two lines whose plain text (inline modifier markup removed) differs compile to the same `voiceKey`, so one recording would voice both — usually because the `voiceKey` template lacks `{prefix}` and collides across documents.",
        spec: &["dsl 0.37.0 §3.6"],
    },
    Code {
        code: "E-ENGINE-IR-VERSION",
        summary: "The engine matrix and execution artifact are outside the accepted IR version line.",
        spec: &["dsl 0.33.0 §4", "dsl 0.33.0 §5"],
    },
    Code {
        code: "E-ENGINE-MATRIX",
        summary: "The engine capability matrix is unreadable or malformed.",
        spec: &["dsl 0.33.0 §4"],
    },
    Code {
        code: "E-ENGINE-OWNED-WRITE",
        summary: "An `::set` writes a state path declared `owner: engine`, though the engine writes such a path and content may only read it.",
        spec: &["dsl 0.22.0 §1.2"],
    },
    Code {
        code: "E-ENGINE-SEMANTICS",
        summary: "The selected engine does not support a semantic capability required by the artifact.",
        spec: &["dsl 0.33.0 §4"],
    },
    Code {
        code: "E-ENTITY-KIND-CLASH",
        summary: "An id is listed as a member of two different entity kinds' `members:`, though an id must belong to exactly one kind unless one kind is declared a `subsetOf:` the other.",
        spec: &["dsl 0.3.0 §3.1", "dsl 0.24.0 §3"],
    },
    Code {
        code: "E-ENTITY-KIND-SHAPE",
        summary: "An entity kind declaration is malformed — it declares neither or both of `members:`/`open:`, has an unknown key, lists a member more than once, or a `labels:`/`add:` entry names a member the kind doesn't have or targets an `open:`/unknown kind.",
        spec: &["dsl 0.3.0 §3.1", "dsl 0.26.0 §2.3", "dsl 0.27.0 §2"],
    },
    Code {
        code: "E-ENTRY-ATTR",
        summary: "A `<entry>` attribute has a malformed shape — a non-quoted-string value, a missing or invalid `id`, or a `series=`/`order=` authored under a document-level `series:`.",
        spec: &["dsl 0.19.0 §3", "dsl 0.19.0 §2.1"],
    },
    Code {
        code: "E-ENTRY-ID-DUP",
        summary: "Two `<entry>` elements share the same `id`, though entry ids must be unique across the document (and project-wide under `check-project`).",
        spec: &["dsl 0.19.0 §3"],
    },
    Code {
        code: "E-ENTRY-SERIES-ORDER",
        summary: "Two `<entry>` elements resolve to the same `(series, order)` position, though each position in a series must name exactly one entry.",
        spec: &["dsl 0.19.0 §2.1", "dsl 0.19.0 §3"],
    },
    Code {
        code: "E-ENTRY-UNREACHABLE",
        summary: "A lore entry's `when` eligibility guard provably never holds, so the entry can never be presented.",
        spec: &["dsl 0.20.0 §5"],
    },
    Code {
        code: "E-ENUM-DEFAULT-NOT-MEMBER",
        summary: "A domain's `default:` value is not one of its own declared members.",
        spec: &["dsl 0.9.0 D-D"],
    },
    Code {
        code: "E-ENUM-EXITS-NOT-MEMBER",
        summary: "A domain's `exits:` list names a value that is not one of its own declared members.",
        spec: &["dsl 0.9.0 D-D"],
    },
    Code {
        code: "E-ENUM-LABEL-NOT-MEMBER",
        summary: "A domain's `labels:` key names a value that is not one of its own declared members.",
        spec: &["dsl 0.24.0 §1"],
    },
    Code {
        code: "E-ENUM-MISSING-SEMANTICS",
        summary: "A domain occupies a slot (such as `anchor`) that requires `default:` or `exits:` semantics, but the domain (or an `entities:` kind standing in for it) declares neither.",
        spec: &["dsl 0.9.0 D-D"],
    },
    Code {
        code: "E-ENUM-UNEXPECTED-SEMANTICS",
        summary: "A domain declares `default:` or `exits:`, but the slot it fills has no meaning for that semantics.",
        spec: &["dsl 0.9.0 D-D"],
    },
    Code {
        code: "E-EXTENDS-RELATION-SIG",
        summary: "An `extends`-base entity kind, relation, or enum is re-declared by a child schema with a members/open shape flip, a missing base member, or a decl that otherwise differs from the base, though a re-declaration must be a legal superset refinement.",
        spec: &["dsl 0.3.0 §4.1"],
    },
    Code {
        code: "E-EXTENDS-STATE-TYPE",
        summary: "An imported schema's state path is re-declared by a deeper `extends` base or the scene's inline `state:` with a different `type`, though persisted state must keep a stable type.",
        spec: &["dsl §9.2"],
    },
    Code {
        code: "E-FACT-DOMAIN",
        summary: "A rule guard, indexed query, or content query compares/binds a relation argument against a variable that ranges over no closed kind, or over a kind that shares no members with the argument's declared domain.",
        spec: &["dsl 0.27.0 §3", "dsl 0.3.0 §3.1"],
    },
    Code {
        code: "E-FACT-EXCLUSIVE",
        summary: "An `::assert{A(x)}` targets arguments where a fact of a mutually exclusive relation already holds, though the two relations are declared exclusive on those arguments.",
        spec: &["dsl 0.25.0 §1"],
    },
    Code {
        code: "E-FACT-QUERY",
        summary: "A fact-query call (`holds`, `count`, `countDistinct`, or `validAt`) does not use the required 0.32 list form `name('relation', ['arg', …])`.",
        spec: &["dsl 0.32.0 §2"],
    },
    Code {
        code: "E-FACT-TIER-WRITE",
        summary: "Content asserts or retracts an `app`-tier base relation, though it is engine-owned/read-only to content, exactly like `app.*` scalar state.",
        spec: &["dsl 0.3.0 §5", "dsl 0.3.0 §9.5"],
    },
    Code {
        code: "E-FLAG-VALUE",
        summary: "A flag attribute (`<choice once>`/`exit`, `<objective optional>`, `<beat also>`) is given a value other than `true`/`false`; a flag is written bare, and a beat/entry `once` period on a choice is refused.",
        spec: &["dsl 0.28.0 §1"],
    },
    Code {
        code: "E-FMT",
        summary: "The formatter cannot parse or preserve the requested file.",
        spec: &["dsl 0.36.0 §1", "dsl 0.36.0 §8"],
    },
    Code {
        code: "E-FMT-CHECK",
        summary: "A formatter check found noncanonical bytes in the requested file.",
        spec: &["dsl 0.36.0 §1", "dsl 0.36.0 §8"],
    },
    Code {
        code: "E-FRONTMATTER-SCHEMA",
        summary: "A document's frontmatter key, declared by an active plugin, holds a value whose shape does not match the plugin's declared type for that key.",
        spec: &[],
    },
    Code {
        code: "E-GRAMMAR-NOT-ADMITTED",
        summary: "A document uses a construct its document kind's grammar forbids in that context — such as a `<quest>` in a scene, `<hub>`/`<timeline>` in a quest body, or staging/choices in a lore `<entry>`.",
        spec: &["dsl 0.2.0 §3.3", "dsl 0.2.0 §6.7", "dsl 0.19.0 §4"],
    },
    Code {
        code: "E-HUB-NO-EXIT",
        summary: "A `<hub>` can never exit: it lacks both an unguarded (`when`-less) `exit` choice and the guarantee that every choice is `once` so the eligible set provably empties.",
        spec: &["dsl §7.3.2", "dsl §11.1.3"],
    },
    Code {
        code: "E-IDENTITY-TEMPLATE",
        summary: "A project's `identity.lineId`/`identity.voiceKey` template names an unknown `{token}`, or resolves to an empty string.",
        spec: &["dsl 0.8.0 §9"],
    },
    Code {
        code: "E-INERT-TITLE",
        summary: "A body `# ` heading: it is no document title. Put the value in frontmatter `title:`, the only document title.",
        spec: &["dsl 0.37.0 §3.1"],
    },
    Code {
        code: "E-INTERP-DEF",
        summary: "A `{{@def}}` interpolation cannot inline into one standalone expression — its body has an expansion cycle, reads `$`, or fails to parse.",
        spec: &[],
    },
    Code {
        code: "E-INTERP-UNTERMINATED",
        summary: "A `{{…}}` interpolation is not closed before the end of the line.",
        spec: &["dsl §7.6"],
    },
    Code {
        code: "E-INTO-TARGET",
        summary: "A `<choice>`'s `into=` attribute is missing, is not a `run.<path>` string literal, or names a bare `run` or non-`run.*` path.",
        spec: &["dsl 0.6.0 §2.2"],
    },
    Code {
        code: "E-INTO-UNDECLARED",
        summary: r#"A `<choice>`'s `into="run.<path>"` names a path that is not declared in the run schema, so it cannot silently create a field."#,
        spec: &["dsl 0.6.0 §2.2"],
    },
    Code {
        code: "E-INTO-VALUE",
        summary: "A `<choice>`'s `into=` run-record sugar has a `value` attribute that is missing, not a `true`/`false` literal for a bool path, or incompatible with the target path's declared type.",
        spec: &["dsl 0.6.0 §2.2"],
    },
    Code {
        code: "E-IR-REMOVED-FIELD",
        summary: "`lute run` received an execution IR that still carries a field 0.37 removed or renamed (such as `addr`, `capabilityVersion`, `recordKey`, `shots`, a flattened `wait`/`duration`/`delay`/`at`/`timeline` stamp, `provenance.injected`, a per-kind field such as camera `zoom`, `cg.full`, `cg`/`video`/`music` `action`, `music.track` or `sfx.name`, an option `label`, or an old line role such as `monologue`), or a command with no `position`. The message names the 0.37 replacement. Recompile the source with the current toolchain.",
        spec: &["dsl 0.37.0 §4", "dsl 0.37.0 §5.3"],
    },
    Code {
        code: "E-JUMP-BACKWARD",
        summary: "A `::jump{to}` names a label that is not forward of the `::jump` in document order — jumps must go forward only.",
        spec: &["dsl 0.12.0"],
    },
    Code {
        code: "E-JUMP-UNDEFINED",
        summary: "A `::jump{to}` names a label that no `::label` anywhere in the document declares.",
        spec: &["dsl 0.12.0"],
    },
    Code {
        code: "E-KIND-MISSING",
        summary: "A root document declares no `kind:` frontmatter key and the manifest's `defaults:` also names none, so the document's kind cannot be resolved.",
        spec: &["dsl 0.2.0 §3.1", "dsl 0.19.0 §2", "dsl 0.10.0 §6.3"],
    },
    Code {
        code: "E-KIND-NAME-CLASH",
        summary: "An `entities:` block declares the same entity kind name twice, two schemas declare one kind differently, or a name is declared as both an entity kind and a relation.",
        spec: &["dsl 0.3.0 §4"],
    },
    Code {
        code: "E-L10N-MODIFIERS",
        summary: "A line translation's inline modifiers (names, span/leaf form, and attributes) differ from its source line's, or its markup does not parse, so the translation is not merged.",
        spec: &["dsl 0.37.0 §6"],
    },
    Code {
        code: "E-LABEL-DUP",
        summary: "A label name — a `::label{name}` — is declared more than once anywhere in the document; every label shares one document-wide namespace.",
        spec: &["dsl 0.12.0"],
    },
    Code {
        code: "E-LEGACY-CONTENT-SIGIL",
        summary: "A content line uses the old `:` speaker sigil, which `@` replaced — write `@speaker{…}: text` instead.",
        spec: &["dsl §7.1"],
    },
    Code {
        code: "E-LINT-CONFIG",
        summary: "`lute.lint.yaml` is malformed — a wrong-typed or unknown top-level key, a bad rule override, or a custom rule id that collides with a core rule id.",
        spec: &[],
    },
    Code {
        code: "E-LINT-EXPR",
        summary: "A lint rule's `when` CEL expression fails to resolve or is mistyped, so that rule is skipped for the document.",
        spec: &[],
    },
    Code {
        code: "E-LINT-RULE",
        summary: "A plugin or custom lint rule declaration is malformed, such as reusing a core rule's id.",
        spec: &[],
    },
    Code {
        code: "E-LOCALE-BUNDLE",
        summary: "`lute compile --locales` failed to merge a locale import file — bad syntax, a row with an empty `locale`, or a duplicate `lineId` for the same locale.",
        spec: &[],
    },
    Code {
        code: "E-LOGIC-CONTENT",
        summary: "A logic block holds a child it does not admit (a `<branch>` or `<hub>` takes only `<choice>`s, a `<match>` only `<when>` and `<otherwise>`), a `<choice>`, `<when>`, `<otherwise>`, `<track>` or `<reward>` stands outside the block it belongs in, or a `<reward>` is not self-closing.",
        spec: &["dsl §7.3", "dsl §7.3.2", "dsl 0.16.0 §2"],
    },
    Code {
        code: "E-LOWER-RECORD-FIELD",
        summary: "A directive's declarative `lower: { record, fields }` mapping names a target field the record does not have, or maps a field it cannot legally write.",
        spec: &[],
    },
    Code {
        code: "E-LOWER-RECORD-UNKNOWN",
        summary: "A directive's `lower: { record: … }` names something outside the closed set of staging record kinds declarative lowering supports.",
        spec: &[],
    },
    Code {
        code: "E-MANIFEST",
        summary: "`lute.project.yaml` cannot be read as a manifest: it does not parse, is not a mapping, lacks `defaultProfile:`, or a value has the wrong shape.",
        spec: &["dsl 0.28.0 §1"],
    },
    Code {
        code: "E-MANIFEST-KEY",
        summary: "`lute.project.yaml` has a key it does not define — at the top level, in a profile, or in `identity:` — or a key another layer owns (a schema key such as `terminal:`, a document key that belongs under `defaults:`).",
        spec: &["dsl 0.28.0 §1"],
    },
    Code {
        code: "E-MATCH-DUP-OTHERWISE",
        summary: "A `<match>` contains more than one `<otherwise>` arm, though at most one is allowed.",
        spec: &["dsl §11.2"],
    },
    Code {
        code: "E-MATCH-NO-SUBJECT",
        summary: "A `<when is=…>` arm sits in a `<match>` with no `subject=`, so its literal has no subject to be compared against; add `subject=` to the `<match>`, or write the arm as `test=`.",
        spec: &[],
    },
    Code {
        code: "E-MATCH-RELATION-SUBJECT",
        summary: "A `<match subject>` subject, directly or via an `@def` it expands to, is a fact query (`holds`/`count`/`validAt`), which match subjects may not be.",
        spec: &["dsl 0.27.0 §2", "dsl 0.3.0 §8"],
    },
    Code {
        code: "E-MAYBE-UNSET",
        summary: "A state path is read where it may not yet be set — no default, no dominating `::set`, and no guard proves it is set.",
        spec: &["dsl §9.4"],
    },
    Code {
        code: "E-META-ID",
        summary: "A document's `id:` frontmatter value is not a dotted id: names (letters, digits, `_` or `-`, not starting with `-`) joined by `.`.",
        spec: &["dsl 0.30.0 §1"],
    },
    Code {
        code: "E-META-MISSING",
        summary: "A required frontmatter key — such as `character`/`season`/`episode`, or an `id:` — is missing from a document that must declare scene identity.",
        spec: &["dsl 0.15.0 §2", "dsl 0.15.0 §4"],
    },
    Code {
        code: "E-META-PARSE",
        summary: "A document's frontmatter is not valid YAML, or parses as YAML but is not a mapping.",
        spec: &[],
    },
    Code {
        code: "E-META-UNKNOWN-KEY",
        summary: "A document declares a top-level meta key that is neither a core key nor owned by an active plugin, or one reserved for another document kind.",
        spec: &[],
    },
    Code {
        code: "E-META-VALUE",
        summary: "A frontmatter value has the wrong shape — a malformed `extra:` mapping or key, a `series:` that is not a name, a malformed `cast:`/`enums:`/`terminal:` entry (in `terminal:`'s long form, a key other than `when`/`persists` or a `persists` that is not `true`/`false`), `persists: true` on a `terminal:` that reads only run state (its ending cannot outlive the run), or a bad `effects:` flag.",
        spec: &["dsl 0.15.0 §3", "dsl 0.19.0 §2.1", "dsl 0.23.0 §7", "dsl 0.29.0 §5"],
    },
    Code {
        code: "E-MISSING-ATTR",
        summary: "A directive is missing an attribute its declared schema requires.",
        spec: &[],
    },
    Code {
        code: "E-MOCK-SUBJECT",
        summary: "A `--mock`/`mocks/*.yaml` entry declares no `file:`, names a `file:` path that does not exist or is not a `.lute` document, or disagrees with the document named on the command line.",
        spec: &["dsl 0.10.0 §8"],
    },
    Code {
        code: "E-MONO-NO-POV",
        summary: "A `{mono}` line's document resolves no point of view (no frontmatter `pov:` and no project `defaults.pov`) and its speaker is not in `monoSpeakers:`. A component line is judged at each `::use` site, in the caller's context.",
        spec: &["dsl 0.37.0 §3.4"],
    },
    Code {
        code: "E-MONO-POV",
        summary: "A `{mono}` line's speaker is neither the document's effective point of view (`pov:`, else the project's `defaults.pov`) nor in its effective `monoSpeakers:` list. A component line is judged at each `::use` site, in the caller's context.",
        spec: &["dsl 0.37.0 §3.4"],
    },
    Code {
        code: "E-NONEXHAUSTIVE",
        summary: "A `<match>` has no `<otherwise>` and its subject's domain is not fully covered by the `<when>` arms — the message names the missing values or the first uncovered gap.",
        spec: &["dsl 0.18.0 §4", "dsl §11.2"],
    },
    Code {
        code: "E-OBJECTIVE-CONTRADICTION",
        summary: "Two required `<objective>`s of one `<quest>` name `done` predicates over the same state path whose solution sets can never both hold.",
        spec: &["dsl 0.10.0 §5.2"],
    },
    Code {
        code: "E-OBJECTIVE-ID-DUP",
        summary: "An `<objective id>` is declared more than once within the same `<quest>`.",
        spec: &[],
    },
    Code {
        code: "E-OBJECTIVE-ID-MISSING",
        summary: "An `<objective>` within a `<quest>` has no `id`.",
        spec: &[],
    },
    Code {
        code: "E-OBJECTIVE-MISSING-DONE",
        summary: "An `<objective>` has an empty `done` completion predicate and no `quest=` reference to delegate its completion.",
        spec: &[],
    },
    Code {
        code: "E-OBJECTIVE-QUEST-DONE",
        summary: "An `<objective>` carries both a `quest=` subquest reference and a non-empty `done=` predicate, which are mutually exclusive.",
        spec: &[],
    },
    Code {
        code: "E-OBJECTIVE-UNSATISFIABLE",
        summary: r#"A required `<objective>`'s `done` predicate can never decide true, or its `<objective quest="…">` references a subquest that is itself unreachable."#,
        spec: &["dsl 0.4.0 §5.3"],
    },
    Code {
        code: "E-OCCASION-GATE",
        summary: "A `lute play` step raises an occasion while its `raisedWhen` gate is false, or after the schema's `terminal:` condition already holds, so the engine would not actually raise it.",
        spec: &["dsl 0.27.0 §4"],
    },
    Code {
        code: "E-OCCASION-UNKNOWN",
        summary: "A beat's or entry's `on=` names an occasion that no resolved plugin declares.",
        spec: &["dsl 0.21.0 §2"],
    },
    Code {
        code: "E-ON-NO-EVENT",
        summary: "An `<on>` has no `event` attribute — every `<on>` must be anchored to a discrete event.",
        spec: &["dsl 0.2.0 §4.1"],
    },
    Code {
        code: "E-PATCH-CHECK",
        summary: "A staged patch introduces a new check or project error.",
        spec: &["dsl 0.36.0 §5.2", "dsl 0.36.0 §8"],
    },
    Code {
        code: "E-PATCH-EDIT",
        summary: "A patch edit is malformed, outside its target, overlaps another edit, or fails its escape-hatch span assertion.",
        spec: &["dsl 0.36.0 §5.1", "dsl 0.36.0 §5.2", "dsl 0.36.0 §8"],
    },
    Code {
        code: "E-PATCH-PRESERVE",
        summary: "A patch violates a requested preserve claim, including an ambiguous reward or component match.",
        spec: &["dsl 0.36.0 §5.3", "dsl 0.36.0 §8"],
    },
    Code {
        code: "E-PATCH-STALE",
        summary: "A patch base project or asserted file revision differs from the current revision.",
        spec: &["dsl 0.36.0 §5.2", "dsl 0.36.0 §8"],
    },
    Code {
        code: "E-PATCH-TARGET",
        summary: "A patch target is unknown, ambiguous, or has no editable source anchor.",
        spec: &["dsl 0.36.0 §5.1", "dsl 0.36.0 §8"],
    },
    Code {
        code: "E-PATH-IDENT",
        summary: "A name has a character outside the name rule (letters, digits, `_` or `-`, not starting with `-`): a state path segment; a quest, objective, entry, branch, hub, choice or mark id; a relation, enum or entity kind; an enum or entity member. A def or def param is not an identifier (a letter or `_`, then letters, digits or `_`): it is read bare, as `@name` or in the def's body. Or a condition writes a name that is not an identifier after a `.` (`quest.zero-coke-001.state`), which CEL reads as a subtraction; the message names the bracket spelling (`quest[\"zero-coke-001\"].state`).",
        spec: &["dsl 0.30.0 §1", "dsl 0.30.0 §2", "dsl §8.4"],
    },
    Code {
        code: "E-PERMISSION-BRIDGE",
        summary: "A directive invokes a bridge service/operation the effective `bridges` permission ceiling forbids.",
        spec: &[],
    },
    Code {
        code: "E-PERMISSION-DIRECTIVE",
        summary: "A `::set`/`::assert`/`::retract` or plugin directive is forbidden by the effective `directives` permission ceiling.",
        spec: &[],
    },
    Code {
        code: "E-PERMISSION-FACT",
        summary: "A fact write — `::assert`/`::retract`, a plugin effect, or a seed fact — targets a relation the effective `factWrites` permission ceiling forbids.",
        spec: &[],
    },
    Code {
        code: "E-PERMISSION-PROFILE",
        summary: "`--permission-profile <name>` was passed without a loaded `lute.project.yaml` from `--project <DIR>` to resolve the profile against.",
        spec: &[],
    },
    Code {
        code: "E-PERMISSION-QUEST",
        summary: "A `<quest>` is declared where the effective `quests` permission ceiling forbids quest declarations.",
        spec: &[],
    },
    Code {
        code: "E-PERMISSION-REWARD",
        summary: "A `<reward>` is declared where the effective `rewards` permission ceiling forbids reward declarations.",
        spec: &[],
    },
    Code {
        code: "E-PERMISSION-STATE",
        summary: "A state write — a `::set`, a choice `into=`, a state/plugin default initialization, or an unresolved plugin write path — targets a path the effective `stateWrites` permission ceiling forbids.",
        spec: &[],
    },
    Code {
        code: "E-PERSIST-REMOVED",
        summary: "A directive uses the removed `persist` attribute — `into=` alone records the run fact.",
        spec: &["dsl 0.6.0 §2.2"],
    },
    Code {
        code: "E-PLUGIN-ASSET-SEGMENT-TYPE",
        summary: "A plugin's `assetKinds` export declares a segment type outside the four a segment position admits — `enum`, `number`, `string`, or `providerRef`.",
        spec: &[],
    },
    Code {
        code: "E-PLUGIN-DUP-ACROSS",
        summary: "Two active plugins declare the same directive, event, reward kind, occasion, cast id, or bridge operation; the first plugin's declaration is used.",
        spec: &[],
    },
    Code {
        code: "E-PLUGIN-DUP-ID",
        summary: "A plugin package declares the same id more than once within one export kind; the first declaration is used and the rest of the project is still checked.",
        spec: &[],
    },
    Code {
        code: "E-PLUGIN-INVALID-DIRECTIVE",
        summary: "A plugin directive declaration uses a `semantics:` flag outside the closed vocabulary, or declares the same attribute name more than once.",
        spec: &[],
    },
    Code {
        code: "E-PLUGIN-IO",
        summary: "A plugin export file or directory could not be read due to an I/O or encoding failure.",
        spec: &[],
    },
    Code {
        code: "E-PLUGIN-KEY",
        summary: "A key in a plugin's `plugin.yaml` or one of its export files is not one that file takes (with the key meant, e.g. `dependencies` → `depends`), or `plugin.yaml` declares a `kind:` other than `capability`.",
        spec: &["dsl 0.28.0 §1"],
    },
    Code {
        code: "E-PLUGIN-MANIFEST",
        summary: "A plugin package's `plugin.yaml` manifest is missing, is not valid YAML, or names no `id`/`version`/`kind`/`exports`.",
        spec: &[],
    },
    Code {
        code: "E-PLUGIN-MISSING-ACTIVE",
        summary: "A profile activates a plugin `id` that is not installed under the plugins directory, or whose package failed to load.",
        spec: &["dsl §11"],
    },
    Code {
        code: "E-PLUGIN-MISSING-EXPORT",
        summary: "A plugin manifest's `exports:` entry names a path that does not exist on disk.",
        spec: &["dsl §4", "dsl §11"],
    },
    Code {
        code: "E-PLUGIN-OPTION-TYPE",
        summary: "A plugin activation's option value does not match the type its manifest declares for that option.",
        spec: &["plugin Appendix C1"],
    },
    Code {
        code: "E-PLUGIN-OPTION-UNKNOWN",
        summary: "A plugin activation sets an option name the plugin's manifest never declares.",
        spec: &["plugin Appendix C1"],
    },
    Code {
        code: "E-PLUGIN-PARSE",
        summary: "A plugin export file is not valid YAML or holds a value of the wrong shape (named with its line and the shape the key takes), or a directive's `effects.writes`/`effects.asserts`/`effects.retracts` entry names an attr the directive never declares (or an assert uses `_`).",
        spec: &["dsl 0.27.0 §2", "dsl 0.27.0 §4", "dsl 0.28.0 §7"],
    },
    Code {
        code: "E-PLUGIN-RESERVED-NAME",
        summary: "A plugin declares a name the core language owns: a directive named like a core statement (`set`, `assert`, `retract`, `accept`, `use`, `body`, `cut`), a core block tag (`scene`, `on`, `quest`, `objective`, `match`, `branch`, `hub`, `choice`, `when`, `otherwise`, `entry`, `beat`, `timeline`, `track`, `reward`, `return`) or a `lute.core` directive (`end`, `mark`, `bg`, …); an event or occasion named like an engine lifecycle event (`questComplete`, …) or a play step key; a cast id `narrator`. Reported at the declaration's file and line.",
        spec: &["dsl §10", "dsl 0.28.0 §1"],
    },
    Code {
        code: "E-PLUGIN-RESERVED-STAMP-ATTR",
        summary: "A non-core plugin's `stampAttrs` export or a directive's `attrs` declares an attribute name (`at`/`duration`/`delay`/`wait`/`timeline`/`provenance`/`source`) that the core stamp already owns.",
        spec: &["plugin §14"],
    },
    Code {
        code: "E-PLUGIN-UNKNOWN-ASSETKIND",
        summary: "A directive binds an attribute to an asset kind that no active plugin declares.",
        spec: &["plugin §7"],
    },
    Code {
        code: "E-PLUGIN-UNKNOWN-EXPORT",
        summary: "A plugin manifest's `exports:` key is not one of the export kinds (with a did-you-mean); the old spellings `rewardkinds`/`assetkinds`/`stampattrs` name `rewardKinds`/`assetKinds`/`stampAttrs`.",
        spec: &["plugin §4", "dsl 0.28.0 §6"],
    },
    Code {
        code: "E-PLUGIN-UNKNOWN-REWARD-TARGET",
        summary: "A `rewardKinds:` entry pins `target: { provider: <name> }` to a provider no active plugin declares.",
        spec: &["dsl 0.16.0 §4"],
    },
    Code {
        code: "E-PLURAL-FORM",
        summary: "A `{{n:plural(…)}}` hint whose forms are not a bare singular and a bare plural separated by `|` — quoted forms, a `,` separator, a missing or empty form.",
        spec: &["dsl 0.27.0 §7", "dsl 0.28.0 §5"],
    },
    Code {
        code: "E-PROFILE-EXTENDS-CYCLE",
        summary: "A profile's `extends` chain loops back to itself.",
        spec: &["dsl §11"],
    },
    Code {
        code: "E-PROFILE-UNKNOWN",
        summary: "The project selects a profile name that `lute.project.yaml` never declares.",
        spec: &["dsl §11"],
    },
    Code {
        code: "E-PROJECT-CONFIG",
        summary: "The editor could not load the document's `lute.project.yaml` (a malformed or unreadable project manifest).",
        spec: &[],
    },
    Code {
        code: "E-QUEST-ID-DUP",
        summary: "A `<quest id=…>` repeats an id already used in this document, across its import graph, or project-wide.",
        spec: &["dsl 0.2.0 §6.3"],
    },
    Code {
        code: "E-QUEST-ID-MISSING",
        summary: "A `<quest>` has no `id` attribute.",
        spec: &["dsl 0.2.0 §6.3"],
    },
    Code {
        code: "E-QUEST-MULTI-PARENT",
        summary: "A subquest is referenced as a child by `<objective quest=…>` from two different parent quests, but a quest may have at most one parent.",
        spec: &[],
    },
    Code {
        code: "E-QUEST-REF-UNKNOWN",
        summary: "An `<objective quest=…>` names a child quest id that no quest in the project defines.",
        spec: &[],
    },
    Code {
        code: "E-QUEST-RESERVED-DECL",
        summary: "A `state:` declaration's path collides with an implicitly-declared reserved quest field (`quest.<id>.*` / `objectives.<oid>.done`).",
        spec: &["dsl 0.2.0 §5.2"],
    },
    Code {
        code: "E-QUEST-RESERVED-WRITE",
        summary: "An `::set` writes a reserved, engine-populated path — `quest.<id>.state`, `objectives.<oid>.done`, an `entry.*` path, `prev.run.*`, `prev.season.*`, or `clock.*`.",
        spec: &["dsl 0.2.0 §5.4", "dsl 0.19.0 §5"],
    },
    Code {
        code: "E-QUEST-TIER-MIX",
        summary: "A subquest's effective `tier` differs from its parent quest's tier.",
        spec: &["dsl 0.23.0 §6"],
    },
    Code {
        code: "E-QUEST-TREE-CYCLE",
        summary: "The parent-child edges induced by `<objective quest=…>` close a cycle, including a quest naming itself as its own child.",
        spec: &[],
    },
    Code {
        code: "E-QUEST-UNREACHABLE",
        summary: "A `<quest>` can provably never complete because its `start` guard always decides false or its `fail` guard always decides true.",
        spec: &["dsl 0.4.0 §5.3"],
    },
    Code {
        code: "E-REF-ARG-TYPE",
        summary: "A `@name(args)` call to a def passes an argument whose static type does not match the def's declared parameter type.",
        spec: &["dsl §8.1"],
    },
    Code {
        code: "E-REF-ARITY",
        summary: "A `@name(args)` call supplies a different number of arguments than the def declares parameters.",
        spec: &["dsl §8.1"],
    },
    Code {
        code: "E-REF-TYPE",
        summary: "A `@ref` produces a type incompatible with the CEL slot, component-arg, or `{{…}}` interpolation position it fills — including a non-renderable produced type or a `:number` format hint on a non-number.",
        spec: &["dsl §8", "dsl §7.6", "dsl 0.24.0 §4"],
    },
    Code {
        code: "E-RELATION-ARITY",
        summary: "A fact atom (a seed `facts:` entry, a rule body/head atom, an `::assert`/`::retract`, or a CEL fact query) supplies a different number of arguments than the relation declares.",
        spec: &["dsl 0.3.0 §4", "dsl 0.3.0 §5"],
    },
    Code {
        code: "E-RELATION-DECL",
        summary: "A relation declares `changedOn:` while not `reserved: true`, `changedOn:` names an occasion no schema declares, or an `excludes:` entry names a relation of incompatible argument kinds or fails its symmetry contract.",
        spec: &["dsl 0.25.0 §1", "dsl 0.25.0 §6"],
    },
    Code {
        code: "E-RELATION-DOMAIN",
        summary: "A relation declares a field the schema does not recognize, an unknown `tier`, an out-of-range/duplicate `key:` index, or an argument domain that names no declared entity kind, enum, or domain.",
        spec: &["dsl 0.3.0 §4"],
    },
    Code {
        code: "E-RELATION-DUP",
        summary: "A relation name is declared more than once in a `relations:` block.",
        spec: &["dsl 0.3.0 §4"],
    },
    Code {
        code: "E-RELATION-EMPTY",
        summary: "A relation declares no `args:`.",
        spec: &["dsl 0.3.0 §4"],
    },
    Code {
        code: "E-RELATION-RESERVED-WRITE",
        summary: "A relation is declared both `derive: true` and `reserved: true`, giving it two conflicting write owners.",
        spec: &["dsl 0.3.0 §4", "dsl 0.3.0 §5"],
    },
    Code {
        code: "E-RELATION-UNKNOWN",
        summary: "A fact atom (a seed, rule, `::assert`/`::retract`, or CEL fact query) names a relation no schema declares.",
        spec: &["dsl 0.3.0 §4"],
    },
    Code {
        code: "E-REMOVED-ATTR",
        summary: "A directive uses a removed attribute with no lossless rewrite (`::music{track}`, `::sfx{name}`); the message names the replacement, and the migration is by hand.",
        spec: &["dsl 0.37.0 §3.3", "dsl 0.37.0 §4"],
    },
    Code {
        code: "E-REMOVED-PROJECT-KEY",
        summary: "`lute.project.yaml` still writes the removed `sequence:` key. It is never read and has no replacement key; every chain is declared in `chapters:`.",
        spec: &["dsl 0.37.0 §2.2"],
    },
    Code {
        code: "E-REMOVED-TAG",
        summary: "A document uses the removed `<scene>` tag. A document's sections are `## ` headings, and its scene is the document itself.",
        spec: &["dsl 0.37.0 §2.2"],
    },
    Code {
        code: "E-RENAME-LEDGER",
        summary: "A rename entry is malformed, has an unknown kind/key, duplicates a source or destination, or is not a canonical key mapping.",
        spec: &["dsl 0.36.0 §3"],
    },
    Code {
        code: "E-RENAME-LEDGER-CYCLE",
        summary: "Rename entries form a chain, self-loop, or cycle.",
        spec: &["dsl 0.36.0 §3"],
    },
    Code {
        code: "E-RENAME-LEDGER-STALE",
        summary: "A ledger source key is still present, or its destination key is absent after project resolution.",
        spec: &["dsl 0.36.0 §3"],
    },
    Code {
        code: "E-RENAMED-ATTR",
        summary: "A directive or content line uses an attribute's old spelling (`::music{action}`, `::video{action}`, `::cg{action}`, `::label{id}`, a content line's `id=`); the message names the new spelling, and `lute fix` rewrites it.",
        spec: &["dsl 0.37.0 §2.2", "dsl 0.37.0 §4"],
    },
    Code {
        code: "E-RENAMED-DIRECTIVE",
        summary: "A directive uses its old tag (`::auto`, `::cut`, `::next`, `::mark`); the message names the new tag (`::actor`, `::cg`, `::jump`, `::label`), and `lute fix` rewrites it.",
        spec: &["dsl 0.37.0 §2.2", "dsl 0.37.0 §4"],
    },
    Code {
        code: "E-RENAMED-TAG-ATTR",
        summary: "A `<choice>` or `<match>` uses an attribute's old spelling (`label=`, `on=`); the message names `text=` or `subject=`, and `lute fix` rewrites it.",
        spec: &["dsl 0.37.0 §3.5", "dsl 0.37.0 §4"],
    },
    Code {
        code: "E-RESERVED-NAME",
        summary: "A declared name is one the language keeps for itself — a state root naming an entity member, def or season, `unset`/`true`/`false`/`null`/`_` naming a member, `none` or a CEL literal naming an id, a CEL keyword in a state path or id that becomes one, a CEL call or rule word naming a relation, `narrator` in `cast:`, or a number in a member list — so the name would be read as that word where it is used. The message names a replacement; `lute --explain E-RESERVED-NAME` lists every reserved name.",
        spec: &["dsl 0.28.0 §1"],
    },
    Code {
        code: "E-RETRACT-WILDCARD-ASSERT",
        summary: "A relation argument is `_` in a context other than a `::retract` pattern, which alone may contain wildcards.",
        spec: &["dsl 0.3.0 §5"],
    },
    Code {
        code: "E-REWARD-ATTR",
        summary: r#"A `<reward>` element is malformed: an empty/missing `kind`, an `amount=` that is not a signed integer or a valid `N..M` range, or an `outcome=` used on an objective-level reward or with a value other than `"failed"`."#,
        spec: &["dsl 0.16.0 §2", "dsl 0.16.0 §6"],
    },
    Code {
        code: "E-REWARD-DUP",
        summary: "Two `<reward>`s of one quest (its own and its objectives') carry the same `id=`; a reward id is unique within its quest.",
        spec: &["dsl 0.37.0 §3.5"],
    },
    Code {
        code: "E-REWARD-KIND",
        summary: "A `<reward kind=…>` value names no reward kind declared in the resolved capability snapshot's `rewardKinds` vocabulary.",
        spec: &["dsl 0.16.0 §4", "dsl 0.16.0 §6"],
    },
    Code {
        code: "E-REWARD-TARGET",
        summary: "A `<reward>`'s `target=` violates its reward kind's target contract: required but missing, or naming neither a declared entity-kind member nor a provider catalog id.",
        spec: &["dsl 0.26.0 §2.5"],
    },
    Code {
        code: "E-RULE-AGGREGATE-CYCLE",
        summary: "A rule's `count(...)`/`countDistinct(...)` aggregate reads a relation that depends on the rule's own head, but an aggregate may only read a relation outside its head's own cycle.",
        spec: &["dsl §9"],
    },
    Code {
        code: "E-RULE-EXCLUSIVE",
        summary: "A rule derives its head relation only where a positive body relation holds on the same arguments, but the two relations are declared mutually exclusive, so every derivation would violate that exclusion.",
        spec: &["dsl 0.25.0 §1"],
    },
    Code {
        code: "E-RULE-GUARD-DEF",
        summary: r#"A rule's `cel("...")` guard cannot expand its `@def`/`@def(args)` references — the def is undeclared, its arity is wrong, or it is otherwise not usable in a guard."#,
        spec: &[],
    },
    Code {
        code: "E-RUN-OWNED-WRITE",
        summary: "`lute run` received an execution IR whose commands write an engine-owned state path or assert/retract a reserved relation.",
        spec: &["dsl 0.32.0 §5"],
    },
    Code {
        code: "E-RUNTIME-BRIDGE-SHAPE",
        summary: "A runtime bridge result omits a field read by the pending bridge call or gives that field a value of the wrong type.",
        spec: &["dsl 0.38.0 §5.3"],
    },
    Code {
        code: "E-RUNTIME-BUSY",
        summary: "A runtime input is not accepted while another input is suspended, or an answer is supplied without a matching await.",
        spec: &["dsl 0.38.0 §5.3"],
    },
    Code {
        code: "E-RUNTIME-HALTED",
        summary: "An input was supplied after the runtime state entered a halted lineage.",
        spec: &["dsl 0.38.0 §5.3"],
    },
    Code {
        code: "E-RUNTIME-INPUT",
        summary: "A runtime seed or input names an unknown project item or contains a value that cannot resolve to its declared type.",
        spec: &["dsl 0.38.0 §5.3"],
    },
    Code {
        code: "E-RUNTIME-IR-VERSION",
        summary: "A runtime bundle declares an execution IR major or minor version the runtime does not support.",
        spec: &["dsl 0.38.0 §4.1"],
    },
    Code {
        code: "E-RUNTIME-OPTION",
        summary: "A runtime choice names an option absent from the pending menu or an option whose verdict is not open.",
        spec: &["dsl 0.38.0 §5.3"],
    },
    Code {
        code: "E-RUNTIME-REQUEST",
        summary: "A runtime choice or bridge result carries a request id different from the pending await.",
        spec: &["dsl 0.38.0 §5.3"],
    },
    Code {
        code: "E-SEASON-DECL",
        summary: r#"A `seasons:` declaration is malformed (an entry that is not a map, a missing or empty `live`, an unknown key, a bad season name), two schemas declare one season differently, or a `season.<name>.*` path, `once: season:<name>` or `tier="season:<name>"` names an undeclared season, or a scene's legacy `season:` key (the episode number) holds a declared season's name; a write to `prev.season.*` is `E-QUEST-RESERVED-WRITE` instead."#,
        spec: &["dsl 0.27.0 §5"],
    },
    Code {
        code: "E-SECTION-DUP",
        summary: "Two sections of one document carry the same `{#id}`; a section id is unique within its document.",
        spec: &["dsl 0.37.0 §3.1"],
    },
    Code {
        code: "E-SECTION-SUFFIX",
        summary: "A section heading has a malformed trailing `{#id}` identity suffix.",
        spec: &["dsl §3.1"],
    },
    Code {
        code: "E-SEMANTICS-UNKNOWN",
        summary: "An execution artifact or engine matrix names an unknown semantic id.",
        spec: &["dsl 0.33.0 §4"],
    },
    Code {
        code: "E-SET-OP-TYPE",
        summary: "An `::set`'s compound operator (`+=`/`-=`/`*=`) targets a path whose declared type is not `number`.",
        spec: &["dsl §7.3.4"],
    },
    Code {
        code: "E-SET-SHAPE",
        summary: "An `::set` is malformed: it has no valid assignment operator (`=`/`+=`/`-=`/`*=`) after the path, uses `==` where `=` was meant, or indexes a state-family path with a param instead of a concrete key.",
        spec: &["dsl 0.27.0 §2"],
    },
    Code {
        code: "E-SET-TYPE",
        summary: "An `::set`'s right-hand expression's decidable type does not match the type declared for the path it writes.",
        spec: &["dsl 0.10.0 §3"],
    },
    Code {
        code: "E-STATE-COLLECTION",
        summary: "A `state:` declaration gives a path a collection type (`list`/`record`/`map`), but author state must be scalar.",
        spec: &["dsl 0.3.0 §3"],
    },
    Code {
        code: "E-STATE-DECL",
        summary: "A `state:` declaration is malformed: a non-string key, an unknown or incomplete `type:` (including an `enum` not nested under `type:`), a bad `default:`/`per:` shape, or `state:` itself is not a map.",
        spec: &["dsl 0.8.0 §4"],
    },
    Code {
        code: "E-STATE-DECL-CONFLICT",
        summary: "Two `state:` declarations of the same path disagree on `type`, `default`, `per`, or `owner`, and neither refines the other via `extends:`.",
        spec: &["dsl §2"],
    },
    Code {
        code: "E-STATE-MAYBE-UNAVAILABLE",
        summary: "A read of a state path is not guaranteed set by any declared `after:` route reaching this node (error grade), or is set on only some of those routes (warning grade).",
        spec: &["dsl §4.3"],
    },
    Code {
        code: "E-STATE-NAMESPACE",
        summary: "A `state:` path does not begin with one of the recognized namespace roots `scene.`, `run.`, `user.`, `app.`, or `season.`.",
        spec: &[],
    },
    Code {
        code: "E-STATE-REDECLARE",
        summary: "A scene's inline `state:` declares or overrides a state path that an imported (`uses:`) schema already declares, which a scene must never redeclare.",
        spec: &["dsl §9.2"],
    },
    Code {
        code: "E-STATE-SHAPE-CYCLE",
        summary: "A `state:` shape refers to itself, directly or through another shape, forming a cycle.",
        spec: &[],
    },
    Code {
        code: "E-STREAM-BODY",
        summary: "A streaming continuation ends mid-construct at the end of input, or its body carries frontmatter or a heading, which a continuation body cannot contain.",
        spec: &[],
    },
    Code {
        code: "E-STREAM-CLOSED",
        summary: "A streaming continuation is submitted after the continuation compiler has already closed.",
        spec: &[],
    },
    Code {
        code: "E-STREAM-PREFIX-CHANGED",
        summary: "Appended source in a streaming continuation would change commands or state that were already emitted for an earlier prefix of the input.",
        spec: &[],
    },
    Code {
        code: "E-STREAM-TEMPLATE",
        summary: "A streaming continuation's template is not a scene with at least one section.",
        spec: &[],
    },
    Code {
        code: "E-STRING-ESCAPE",
        summary: r#"A quoted attribute value uses a backslash escape other than the four defined ones (`\"`, `\\`, `\n`, `\t`)."#,
        spec: &["dsl §4.4"],
    },
    Code {
        code: "E-SUBQUEST-REARM",
        summary: "A quest that an `<objective quest=…>` names as a subquest declares `rearm=`; a subquest activates with its parent, so once the parent has ended a rearmed child stays `unset`.",
        spec: &["dsl 0.27.0 §5", "dsl 0.28.0 §5"],
    },
    Code {
        code: "E-TAG-INLINE-BODY",
        summary: "A block's body, and often its close, is written on the opener's own line; the opener, each body line and the `</tag>` close each need a line of their own.",
        spec: &["dsl §2.3"],
    },
    Code {
        code: "E-TAG-NOT-ONE-LINE",
        summary: "A `<tag …>` opener's attributes wrap past its own physical line instead of staying on one line as the grammar requires.",
        spec: &["dsl §2.3"],
    },
    Code {
        code: "E-TASK-TRAP",
        summary: "An edit-task suite trap did not refuse or flag the patch as expected.",
        spec: &["dsl 0.36.0 §6", "dsl 0.36.0 §8"],
    },
    Code {
        code: "E-TEMPLATE",
        summary: "A beat template is misused: `<beat use=>` names no component or one without a `beat:` header, a template `beat:` header is malformed or gives a header param a value it cannot take, a component declares a param named like a key of its use (`component` or `when`, or for a beat template a `<beat>` header key such as `title`, `once` or `id`) that no use could ever pass, or `::body` appears outside a template's top level.",
        spec: &["dsl 0.27.0 §6", "dsl 0.28.0 §1"],
    },
    Code {
        code: "E-TEMPORAL-ARG",
        summary: "A narrative-time value (`now()` etc.) is used somewhere other than an ordering comparison against another narrative-time value or `validAt`'s second argument — as a bare value, in arithmetic, indexing, field access, a list literal, or with `!=`.",
        spec: &["dsl 0.3.0 §6"],
    },
    Code {
        code: "E-TEST-FILE",
        summary: "A `*.test.yaml`'s `file:` names a document that does not exist.",
        spec: &[],
    },
    Code {
        code: "E-TEST-KEY",
        summary: "A `*.test.yaml` has an unrecognized top-level or `expect:`-level key, or a key that is not a string.",
        spec: &[],
    },
    Code {
        code: "E-TEST-LORE",
        summary: "A test's `file:` names a lore document, which is looked up rather than played, so the test must instead name what to present (`entry:`/`entries:`, `beat:`) or judge it with `expect:`.",
        spec: &["dsl 0.22.0 §5"],
    },
    Code {
        code: "E-TEST-NEEDLE",
        summary: "A `*.test.yaml`'s `transcriptContains`/`transcriptLacks` needle names a speaker outside the project's cast, an attribute no transcript line shows, or a value outside its domain, so it could never match a presented line.",
        spec: &[],
    },
    Code {
        code: "E-TEST-NO-EXPECT",
        summary: "A `*.test.yaml` declares no recognized `expect:` key, so the test asserts nothing and cannot pass.",
        spec: &[],
    },
    Code {
        code: "E-TEXT-ESCAPE",
        summary: "An inline text modifier uses a backslash escape that is not one of the defined punctuation escapes.",
        spec: &["dsl §3.6"],
    },
    Code {
        code: "E-TEXT-MODIFIER",
        summary: "An inline text modifier has malformed nesting or an unterminated span or attribute body.",
        spec: &["dsl §3.6"],
    },
    Code {
        code: "E-TIME-RESOLUTION",
        summary: "An authored time value (a clip `at`, `duration`, `delay`, or `<timeline duration>`) carries more fractional precision than a millisecond.",
        spec: &["dsl 0.10.0 §10.1"],
    },
    Code {
        code: "E-TIMELINE-CONTENT",
        summary: "A `<timeline>` or `<track>` body contains non-staging content instead of only clip/staging elements.",
        spec: &["dsl §7.4"],
    },
    Code {
        code: "E-TIMELINE-DURATION",
        summary: "A `<timeline duration>` is explicitly set below the maximum resolved end of its clips, which would truncate the timeline's own content.",
        spec: &["dsl §11.4"],
    },
    Code {
        code: "E-TRACE-ACCEPT",
        summary: "A `--accept`/`accept:` entry names an unknown quest id, a quest that carries a `start` predicate (which activates declaratively, needing no accept), or a quest referenced by a parent's `<objective quest=…>` (a no-start child that activates via its parent).",
        spec: &["dsl 0.4.0 §4.3", "dsl 0.4.0 §4.4"],
    },
    Code {
        code: "E-TRACE-BEAT",
        summary: "`lute trace --beat <id>` targets a document that is not `kind: lore`, or names an id no `<beat>` in the document declares.",
        spec: &["dsl 0.23.0 §4"],
    },
    Code {
        code: "E-TRACE-CHOICE",
        summary: "A `--choose` entry names an unknown branch/hub id or an unknown choice id for that branch/hub, either before the walk starts or because the choice's guard decides false when reached.",
        spec: &["dsl 0.4.0 §4.3", "dsl 0.4.0 §4.4"],
    },
    Code {
        code: "E-TRACE-ENTRY",
        summary: "`lute trace --entry <id>` targets a document that is not `kind: lore`, or names an id no `<entry>` in the document declares.",
        spec: &["dsl 0.19.0 §8"],
    },
    Code {
        code: "E-TRACE-EVENT",
        summary: "A `--event`/`events:` entry names a built-in lifecycle event (`questActive`, `questComplete`, `questFailed`), which is engine-derived and can never be user-fired.",
        spec: &["dsl 0.4.0 §4.3", "dsl 0.4.0 §4.4"],
    },
    Code {
        code: "E-TRACE-MOCK-FACT",
        summary: "A `--fact`/`facts:` entry, or a test's `expect.facts`/`expect.notFacts` atom, does not parse as a ground fact pattern, or names an unknown relation, wrong arity, or a foreign argument.",
        spec: &["dsl 0.4.0 §4.3"],
    },
    Code {
        code: "E-TRACE-MOCK-PARSE",
        summary: "A `--mock`/`mocks/*.yaml` file is malformed — invalid YAML, not a mapping, an unrecognized top-level key, or a `state:`/`facts:`/`choose:`/`events:`/`quests:` section with the wrong shape.",
        spec: &["dsl 0.4.0 §4.3", "dsl 0.10.0 §8"],
    },
    Code {
        code: "E-TRACE-MOCK-TYPE",
        summary: "A mock or test seed's literal (`--state <path>=<literal>`, `state:`, `quests:`) or a test's `expect.state` value is not compatible with the path's reserved domain or declared type — for a `{ domain: K }`, `{ entity: K }` or enum path, not one of its members — or an answered bridge result lacks a field that content reads.",
        spec: &["dsl 0.4.0 §4.3", "dsl 0.24.0 §5", "dsl 0.27.0 §2"],
    },
    Code {
        code: "E-TRACE-MOCK-UNDECLARED",
        summary: "A `--state <path>=…` seed names a path the clock derives (not seedable), a path not declared in the resolved schema, a path nothing in the document reads, or a bridge answer naming a call/field no directive reads or writes.",
        spec: &["dsl 0.4.0 §4.3", "dsl 0.24.0 §1", "dsl 0.24.0 §5"],
    },
    Code {
        code: "E-TRACK-KEY",
        summary: "A `<track>` declares neither `subject`, `channel`, nor a `subject`+`property` pair, so it has no identifying key.",
        spec: &["dsl §7.4"],
    },
    Code {
        code: "E-UNCLASSIFIED",
        summary: "A body line is no Lute construct — not a content line, directive, `::set` or known block — or a block stands where it cannot, such as a `<quest>` inside a section.",
        spec: &["dsl 0.5.0 §2.1"],
    },
    Code {
        code: "E-UNCLOSED-TAG",
        summary: "A block is never closed — its body reaches the end of the file, a `## ` heading, or the close of an enclosing block before its own `</tag>` — or a `</tag>` closes no open block.",
        spec: &["dsl §5", "dsl §7.3"],
    },
    Code {
        code: "E-UNDECLARED",
        summary: "A CEL slot, `::set` target, or rule guard reads or writes a state path that no schema declares.",
        spec: &["dsl §7.3.4", "dsl §9.4"],
    },
    Code {
        code: "E-UNDECLARED-REF",
        summary: "A `@name` interpolation or guard reference names a `def` that no schema declares.",
        spec: &["dsl §8.1"],
    },
    Code {
        code: "E-UNKNOWN-ATTR",
        summary: "A content line or directive carries an attribute key that the content-line grammar or the directive's own declaration does not recognize.",
        spec: &["dsl 0.1.0 §7.1"],
    },
    Code {
        code: "E-UNKNOWN-DIRECTIVE",
        summary: "A `::directive` names a tag no core or active plugin declares.",
        spec: &[],
    },
    Code {
        code: "E-UNKNOWN-EVENT",
        summary: r#"An `<on event="…">` names an event that resolves to neither a built-in lifecycle event nor a capability-declared world event."#,
        spec: &["dsl 0.2.0 §4.5"],
    },
    Code {
        code: "E-UNKNOWN-ID",
        summary: "An attribute referencing a `providerRef` id names an id absent from the pinned provider catalog.",
        spec: &[],
    },
    Code {
        code: "E-UNKNOWN-KIND",
        summary: "A document's `kind:` frontmatter key has a value other than `scene`, `quest`, or `lore`.",
        spec: &["dsl 0.2.0 §3.1", "dsl 0.19.0 §2"],
    },
    Code {
        code: "E-UNSET-LITERAL",
        summary: "A CEL guard slot compares a maybe-unset finite-domain subject to the foreign string literal `'unset'`, the most common misspelling of the DSL's actual unset sentinel.",
        spec: &["dsl 0.5.2 §2"],
    },
    Code {
        code: "E-UNSET-UNCOVERED",
        summary: "A `<match>` subject that may be unset (a `run.`/`user.`/`app.` path with no schema `default`; a `scene.*` path, including a branch's `scene.choices.*` record, is judged per path as `E-MAYBE-UNSET`) is not covered by an `unset`-matching arm or an `<otherwise>`.",
        spec: &["dsl §11.2"],
    },
    Code {
        code: "E-USES-CYCLE",
        summary: "A document's `uses:`/`extends:` imports form a directed cycle.",
        spec: &[],
    },
    Code {
        code: "E-USES-DUP-DEF",
        summary: "Two peer imports at the same import depth declare the same `def` name differently.",
        spec: &[],
    },
    Code {
        code: "E-USES-DUP-RELATION",
        summary: "Two peer imports at the same import depth declare the same relation or enum name differently.",
        spec: &[],
    },
    Code {
        code: "E-USES-DUP-STATE",
        summary: "Two peer imports at the same import depth declare the same state path differently.",
        spec: &[],
    },
    Code {
        code: "E-USES-NOT-FOUND",
        summary: "A `uses:`/`extends:` import names a path that cannot be resolved or read.",
        spec: &[],
    },
    Code {
        code: "E-USES-PARSE",
        summary: "A `uses:`/`extends:` import's target document has parse or frontmatter errors of its own.",
        spec: &[],
    },
    Code {
        code: "E-VALIDAT-DERIVED",
        summary: "`validAt` is used over a derived relation, whose rule closure carries a CEL guard and so keeps no single well-defined timestamp.",
        spec: &["dsl §8"],
    },
    Code {
        code: "E-WHEN-LITERAL-DOMAIN",
        summary: r#"A `<when is="…">` literal falls outside the subject's decided finite domain — a foreign enum member (a typo), a number/bool literal against a mismatched domain, or `unset` on a subject that is never unset."#,
        spec: &["dsl 0.4.0 §5.2", "dsl §6.3"],
    },
    Code {
        code: "E-WHEN-PATTERN",
        summary: "A `<when>` arm carries neither an `is` literal pattern nor a `test` guard, but one of the two is required.",
        spec: &["dsl §7.3.1"],
    },
    Code {
        code: "E-WHEN-RANGE",
        summary: r#"A `<when is="…">` alternative contains `..` but is a malformed or empty range literal (for example `..`, `a..b`, `1...2`, or `3..1`)."#,
        spec: &["dsl 0.18.0 §2"],
    },
    Code {
        code: "E-WHEN-UNSET-SUBJECT",
        summary: r#"A `<when is="unset">` arm needs its `<match>` subject to be a plain state path, and this subject is an expression."#,
        spec: &["dsl §7.3.1"],
    },
    Code {
        code: "E-WRITE-CONFLICT",
        summary: "Two `<clip>`s on different `<track>`s of a `<timeline>` write overlapping state targets at overlapping times.",
        spec: &["dsl §11.4"],
    },
    Code {
        code: "W-ASSET-PLACEHOLDER",
        summary: "An asset id looks like a placeholder that should be resolved before release.",
        spec: &[],
    },
    Code {
        code: "W-BEAT-ONCE-RUN-USER",
        summary: "A scene beat's `once` defaults to `run`, but its `when` reads only user-tier state, so it may replay on every run unless `once` is authored explicitly.",
        spec: &["dsl 0.22.0 §13", "dsl 0.23.1"],
    },
    Code {
        code: "W-BEAT-PRIORITY-TIE",
        summary: "Two or more beats on one `select: first` occasion share the same `priority` and can be eligible at the same time, so which one wins falls to file order.",
        spec: &["dsl 0.27.0 §11", "dsl 0.22.0 §13"],
    },
    Code {
        code: "W-BEAT-SHADOWED",
        summary: "A `select: first` beat can never win its occasion because an earlier-ordered, always-eligible, never-spent beat with the same or absent target always wins first.",
        spec: &["dsl 0.21.0 §5"],
    },
    Code {
        code: "W-BEAT-SPENT-AT-START",
        summary: "A beat's `spentBy` already holds at the start of play (every state path at its default, only the seed facts, each quest `unset` until its `start` holds — a `start=\"true\"` quest is already `active`) — often `spentBy` read as \"repeat while\", or an inverted `!holds(…)` copied from an old `when` — so the beat is spent before it can play: a `spentBy` beat stays spent once its condition has held. The message names the rewrite: `when: \"!(…)\"` (with `once: false` to repeat) for a beat that plays while the condition does not hold.",
        spec: &["dsl 0.27.0 §5", "dsl 0.28.0 §6"],
    },
    Code {
        code: "W-BEAT-UNRAISED",
        summary: "A beat answers an occasion the clock's `raise:` map raises, and its `when` can hold only where the clock does not raise it — a `dayEnd` beat for a slot other than the day's last, a `dayStart` beat for a later slot of the day the run starts — or only at the last `dayEnd`, which the advance that ends the clock raises after `clock.ended` turns true, when a `terminal:` that holds with `clock.ended` has already ended the game. So it never plays. Answer an occasion raised where it holds, or change its `when`; for the last `dayEnd`, raise it before the clock ends or write `terminal:` so it holds only once the beat has played. A beat whose `when` holds where the run starts — the slot occasion or `dayStart` there, which the clock does not raise because no `advance:` stops there — is warned too, but it plays if the engine raises the occasion when a run starts: then declare `raiseAtStart: true` on the clock, which silences it; otherwise answer an occasion raised where it holds.",
        spec: &["dsl 0.28.0"],
    },
    Code {
        code: "W-BRANCH-ID-SHARED",
        summary: "Two documents of one project each declare a `<branch>` or `<hub>` with the same id. Ids need only be unique within a document, but a play's or test's `choose:` names a menu by its id alone, so one key answers both menus (and a list of decisions is consumed across both).",
        spec: &["dsl 0.28.0 §7"],
    },
    Code {
        code: "W-CAST-ABSENT",
        summary: "A content line's speaker has a cast entry declaring a `present:` condition, but the line's enclosing guards do not imply that condition holds.",
        spec: &["dsl 0.24.0 §4"],
    },
    Code {
        code: "W-CATALOG-STALE",
        summary: "A `providerRef` id is not found in the pinned provider catalog, which may mean the snapshot is stale or offline rather than the id being wrong.",
        spec: &["dsl §7.2"],
    },
    Code {
        code: "W-CHAPTER-ORDER",
        summary: "On a `select: sequence` occasion, where a chain of the project's `chapters:` is the order within one raise, a listed scene writes its own `priority:`, which places it out of the order the chain lists. Remove the scene's `priority:`, or move it in the chain's `scenes:`.",
        spec: &["dsl 0.28.0 §4"],
    },
    Code {
        code: "W-CHAPTER-STALL",
        summary: "A scene listed in a chain of the project's `chapters:` has its own `when:` that can stay false for good — it reads state the story may never set, or a window of the clock that closes: a `when` no raise of the chain's occasion meets (a `dayStart` chain whose scene needs the day the run starts, unless the clock declares `raiseAtStart: true` because the engine raises it there), or one that no later raise meets once the scene before it has played late (a slot of the last day of a clock that ends) — and the next listed scene's `after:` (the one the chain writes, or one it wrote itself) waits on it, so the chapters can stop there. A clock condition that a later raise still meets only delays the chain, and a condition over other `owner: engine` state is the engine's to make true; neither is reported. Only a chain stalls: a hand-written `after:` on a scene outside `chapters:` is itself the statement \"wait for that scene\" and never warns. If the scene may be skipped, let the next one follow the scene before it (`after: visited(\"<previous>\")`; the skipped one still plays first while eligible, as it ranks higher); if it must play, make sure the story makes its condition true, or let its `when` hold at a later raise.",
        spec: &["dsl 0.28.0 §4", "dsl 0.29.0 §3"],
    },
    Code {
        code: "W-CODE-AFTER-END",
        summary: "Content follows an `::end` directive in the same straight-line body, but the walk already terminated there so nothing after it can run.",
        spec: &["dsl 0.8.0"],
    },
    Code {
        code: "W-CODE-AFTER-JUMP",
        summary: "Content follows an unguarded `::jump` directive in the same straight-line body, but the jump leaves that body so nothing after it can run.",
        spec: &["dsl 0.12.0"],
    },
    Code {
        code: "W-COMPONENT-INSTANCE-UNTAGGED",
        summary: "With `identity.requireStable`, a `::use` has no explicit `instance` and is using the positional fallback; run `lute tag` to write one.",
        spec: &["dsl 0.36.0 §2.2"],
    },
    Code {
        code: "W-COMPONENT-UNVERIFIED",
        summary: "A standalone component check has no caller in scope — either no project was resolved, or the resolved project has no document that `::use`s the component — so the verdict covers only the component's own frontmatter and body.",
        spec: &["dsl 0.10.0 §9"],
    },
    Code {
        code: "W-DEADLINE-BEFORE-DONE",
        summary: "An `on=` objective's `by=` deadline (with no `until=`) provably implies before its `done` predicate can ever be judged, so the deadline fails the objective before it can complete.",
        spec: &["dsl 0.24.0 §2.1"],
    },
    Code {
        code: "W-DEADLINE-BEFORE-WINDOW",
        summary: "An objective's `done` can only hold at clock positions where its `by=` deadline already holds — typically a `visited` beat whose `when` opens after the deadline — so the deadline fails the objective before it can be done.",
        spec: &["dsl 0.28.0"],
    },
    Code {
        code: "W-DEADLINE-NEVER",
        summary: "An objective's `by=` deadline can never hold — typically a moment past the end of a clock that ends — so it never fails the objective.",
        spec: &["dsl 0.24.0 §2.1"],
    },
    Code {
        code: "W-DEF-UNUSED",
        summary: "A declared `@def` is never referenced by an `@name` use anywhere in the project's content, other defs, or rule guards.",
        spec: &["dsl 0.24.0"],
    },
    Code {
        code: "W-DERIVE-NO-RULES",
        summary: "A relation declared `derive: true` has no rules that produce it, so it is legal but permanently empty — almost always a typo'd rule head.",
        spec: &["dsl 0.3.0 §7.1"],
    },
    Code {
        code: "W-DISPLAY-NAME-DUP",
        summary: "Two different speakers show the same dialogue display name (from a cast entry's `name:` or a component `::use{name=}`), so the player cannot tell them apart.",
        spec: &["dsl 0.26.0 §2.8"],
    },
    Code {
        code: "W-DOMAIN-UNREAD",
        summary: "A declared domain is not read by any active construct — no directive attribute, content-line slot, state path, `relations:` argument, `per:`/`subsetOf:` family, or rule/condition query names it — so it enforces nothing.",
        spec: &["dsl 0.10.0 §11.1"],
    },
    Code {
        code: "W-ENTRY-REF-UNKNOWN",
        summary: "An `entry.<id>.read` reference names an entry id no lore document in the project declares.",
        spec: &["dsl 0.19.0 §5"],
    },
    Code {
        code: "W-ENTRY-WRITE-REREAD",
        summary: "An entry that can be read again in a run (a lookup entry, an entry beat without `once`, a `once` shorter than the run, a `spentBy` entry, a `for=` entry without `once: run|user`) writes state, but an entry's writes apply on its first read in a run only; the message names the remedy for its shape (a `<beat>` with the same attributes, `once=\"run\"`, or a `when=\"!entry.<id>.read\"` guard, which also silences it).",
        spec: &["dsl 0.26.0 §8", "dsl 0.19.0 §6"],
    },
    Code {
        code: "W-EXIT-INERT",
        summary: "A content line's `action` names a declared exit member of the `action` domain, but on a content line it has no staging effect — the character stays on stage.",
        spec: &["dsl 0.10.0 §11.2"],
    },
    Code {
        code: "W-FACT-GUARANTEED",
        summary: "A guard's relational query (`holds`/`count`) is guaranteed true on every route that reaches it, making the condition redundant.",
        spec: &["dsl 0.20.0 §5"],
    },
    Code {
        code: "W-INTO-SET-DUP",
        summary: "A `<choice>` arm both `::set`s a path and records the same path via `into=`, recording it twice.",
        spec: &["dsl 0.6.0 §2.2"],
    },
    Code {
        code: "W-L10N-MISSING",
        summary: "A compiled line record is missing text for a locale its localization bundle declares.",
        spec: &["dsl 0.8.0 §7"],
    },
    Code {
        code: "W-LINE-CODE-UNTAGGED",
        summary: "With `identity.requireStable`, a content line has no explicit per-speaker `code` and compiler allocation is being used; run `lute tag` to write one.",
        spec: &["dsl 0.36.0 §2.2"],
    },
    Code {
        code: "W-LUTE-VERSION-STALE",
        summary: "A document's `luteVersion` frontmatter stamp is present but differs from the toolchain's current DSL version, suggesting it was copied from an older example.",
        spec: &["dsl 0.6.1 §3"],
    },
    Code {
        code: "W-META-LEGACY",
        summary: "A frontmatter document authors a legacy scene-identity key (such as `character`, `season`, or `episode`) alongside `id:`, but `id:` now carries scene identity so the legacy key should be removed.",
        spec: &["dsl 0.15.0 §4"],
    },
    Code {
        code: "W-OBJECTIVE-HIDDEN",
        summary: "A required (`!optional`) objective's `visibleWhen` visibility gate provably never holds, so it can never be visible or tracked even though it still gates quest completion.",
        spec: &["dsl 0.4.0 §5.3"],
    },
    Code {
        code: "W-OBJECTIVE-STRANDED",
        summary: "Within the declared clock windows, a required objective may be stranded because its candidate beats' windows can all close; no path search is performed.",
        spec: &["dsl 0.31.0 §3"],
    },
    Code {
        code: "W-OTHERWISE-DEAD",
        summary: "A `<match>`'s `<otherwise>` arm is provably unreachable because earlier unguarded `is` arms already cover the subject's whole domain.",
        spec: &["dsl 0.4.0 §5.2"],
    },
    Code {
        code: "W-OVERLAP-ARMS",
        summary: "Two `<when>` arms provably match the same value, so the later arm is unreachable under first-match-wins ordering.",
        spec: &["dsl §11.2", "dsl 0.18.0 §4"],
    },
    Code {
        code: "W-PROJECT-INERT",
        summary: "A manifest does not govern under the forced `--project` root and would have resolved differently, so its settings are not applied to any document.",
        spec: &[],
    },
    Code {
        code: "W-QUEST-HANDLER-DEAD",
        summary: r#"A quest's `<on event="questFailed">` handler never runs because the quest can never fail — no `fail` condition, no required objective with a `by=` deadline, no failing required subquest, and no parent quest that could cascade-fail it."#,
        spec: &["dsl 0.22.0 §7"],
    },
    Code {
        code: "W-QUEST-NEVER-ACCEPTED",
        summary: r#"An accept-driven quest is never named by any `::accept{quest=…}` and is not `accept="external"`, so nothing in the project ever accepts it."#,
        spec: &["dsl 0.24.0 §2", "dsl 0.25.0 §5"],
    },
    Code {
        code: "W-QUEST-REARM-CONSTANT",
        summary: "A quest's `rearm=` condition is constant (`\"true\"`, `\"false\"`, or a def or comparison that folds to one), so it never turns from false to true and the quest never rearms.",
        spec: &["dsl 0.27.0 §5", "dsl 0.28.0 §5"],
    },
    Code {
        code: "W-QUEST-REF-UNKNOWN",
        summary: "A reserved `quest.<id>.state` / `quest.<id>.objectives.<oid>.done` reference (or similar) names a quest id or objective id no quest document in the project defines.",
        spec: &["dsl 0.5.1 §1.4"],
    },
    Code {
        code: "W-QUEST-STATE-HAS",
        summary: "A `has(quest.<id>.state)` guard is always true, since a quest's state is always assigned — `unset` until activation, then a real state — so the check tests nothing.",
        spec: &[],
    },
    Code {
        code: "W-QUEST-TIER-IMPLICIT",
        summary: "A quest with no `tier=` (so it defaults to user-tier, persisting across runs) reads only run-tier state in its conditions — `run.*`, `clock.*` over a `run.*` day, run-tier facts, or subquests that are run-tier or flagged too, but not `visited()`, which a new run keeps — suggesting it (and its quest tree) was meant to reset each run.",
        spec: &[],
    },
    Code {
        code: "W-RELATION-TIER-IMPLICIT",
        summary: "A stored (not `derive: true`) relation declares no `tier:`, so it is run-tier: its facts start over at every new run, from its `facts:` seed if any (an engine-owned `reserved: true` relation's facts are forgotten). Write `tier: run` to keep that, or `user`, `app` or `season:<name>` for facts that outlive the run.",
        spec: &["dsl 0.28.0 §5"],
    },
    Code {
        code: "W-RELATION-UNREAD",
        summary: "A declared, non-reserved relation is written (asserted, seeded, or derived) but never read by any condition, rule body, or def — the facts it records change nothing.",
        spec: &["dsl 0.24.0"],
    },
    Code {
        code: "W-REWARD-DOUBLE-CREDIT",
        summary: r#"A quest handler's `::set` writes the same path a `<reward kind="…" credits=…>` already credits when granted, so the reward pays twice."#,
        spec: &["dsl 0.23.0 §8"],
    },
    Code {
        code: "W-SEASON-UNGATED",
        summary: "A beat with `once: season:<name>` (or a `tier=\"season:<name>\"` quest with a `start`) whose `when` (or `start`) does not imply the season's `live` condition: `once` only sets how long the beat stays spent, so it plays even while the season has never opened. Add the season's `live` condition (or a def that reads it) to the `when`.",
        spec: &["dsl 0.28.0 §7"],
    },
    Code {
        code: "W-SLOT-CONTENTION",
        summary: "Within the declared clock windows, two required objectives may contend for the same single clock position; no path search is performed.",
        spec: &["dsl 0.31.0 §4"],
    },
    Code {
        code: "W-SPENT-BY-REVERSIBLE",
        summary: "A beat's `spentBy` can turn false again after it has held — it reads a fact some `::retract` / `::assert` or a directive's declared effect can undo, a season's state, facts or quest (reset each time the season opens), or a quest `rearm` returns to `unset` — but with no `once` written the beat stays spent for the rest of the run, unlike a condition judged afresh at each raise. The message names the rewrite: `once: false` with `when: \"!(…)\"` to judge it afresh, `once: season:<name>` for a season, or `once: run` to keep it spent on purpose (a written `once` silences the warning).",
        spec: &["dsl 0.28.0 §6"],
    },
    Code {
        code: "W-STAGE-ABSENT",
        summary: "A content line or `::actor` targets a character who already left the stage (via a declared exit, a `::bg` scene change, or `::clear`) and was never re-shown, so the staging is impossible.",
        spec: &["dsl 0.22.0 §12"],
    },
    Code {
        code: "W-TEMPLATE-DOT-PARAM",
        summary: "A beat template's `when:` or `spentBy:` header reads a member as a path segment spelled with a param (`user.bond.@who`). It works in a header, but a component body refuses that spelling; write `user.bond[@who]`, which both accept.",
        spec: &["dsl 0.28.0 §3"],
    },
    Code {
        code: "W-TEMPLATE-OVERRIDE",
        summary: "A `<beat use=…>` writes its own `when=`, which replaces the template's `when:` whole, so the template's condition no longer gates the beat (and an argument only that condition read is unused). Write both conditions in the use's `when=`, or give the template a param to conjoin (`when: \"<condition> && (@only)\"`) and pass it instead.",
        spec: &["dsl 0.28.0 §3"],
    },
    Code {
        code: "W-TERMINAL-PERSISTENT",
        summary: "The schema's `terminal:` reads state a new run keeps (`visited(…)`, `user.*`, `app.*`, `entry.<id>.everRead`, a user-tier quest or relation), so once it holds no new run can play on. An ending that outlives runs on purpose (a roguelike's permanent ending) says so with `terminal: { when: \"<condition>\", persists: true }`, which silences the warning.",
        spec: &["dsl 0.28.0 §2", "dsl 0.29.0 §5"],
    },
    Code {
        code: "W-TEXT-BRACKET-LABEL",
        summary: r#"A `<choice>` label is wholly wrapped in `[…]`, Ink's bracket suppression. Lute shows a label exactly as written, so the brackets appear on the button; write the label without them. A leading tag followed by more text (`[Persuasion] Step closer`) is not warned."#,
        spec: &["dsl 0.28.0 §2"],
    },
    Code {
        code: "W-TEXT-COMMENT-LIKE",
        summary: "Line text or a choice label holds a ` // …` comment or ends in an Ink `#tag`. Text after `: ` is literal, so the player sees it; a comment is `// …` on a line of its own, and Lute has no line tags.",
        spec: &["dsl 0.28.0 §2"],
    },
    Code {
        code: "W-TEXT-GLUE",
        summary: "Line text or a choice label holds Ink glue `<>`. Lute joins no lines and text after `: ` is literal, so the player sees `<>`; write the whole sentence on one line.",
        spec: &["dsl 0.28.0 §2"],
    },
    Code {
        code: "W-TEXT-LOOKS-LIKE-REF",
        summary: r#"A content line's whole text is exactly `@<name>` for a declared def or component param, which ships as the literal string `"@<name>"` instead of being resolved."#,
        spec: &["dsl §7.6"],
    },
    Code {
        code: "W-TEXT-SINGLE-BRACE",
        summary: "Line text or a choice label holds a single-brace group that reads as another language's markup: a state path or def (`{run.oil}`), a Yarn `{$var}` or `{0}` placeholder, Ink conditional text (`{cond: text}`) or alternatives (`{~a|b}`). Single braces are always literal and a backslash does not escape them (`\\{run.oil\\}` ships its backslashes too), so braces meant to show are written as they are; interpolation is `{{run.oil}}`, and conditional text is a guarded line or a `<match>`.",
        spec: &["dsl 0.28.0 §2"],
    },
    Code {
        code: "W-TIMELINE-CLIPS",
        summary: "A `<timeline>` track has more than 12 clips, which the checker suggests splitting.",
        spec: &["dsl §11.4"],
    },
    Code {
        code: "W-TIMELINE-TOTAL",
        summary: "A `<timeline>` has more than 40 clips across all its tracks combined, which the checker suggests splitting.",
        spec: &["dsl §11.4"],
    },
    Code {
        code: "W-TIMELINE-TRACKS",
        summary: "A `<timeline>` has more than 8 tracks, which the checker suggests splitting.",
        spec: &["dsl §11.4"],
    },
    Code {
        code: "W-TRACE-MOCK-UNPRODUCIBLE",
        summary: "A supplied `--fact`/mock-YAML fact's relation is judged not producible by any authored producer, so a walk seeded with it proves nothing about reachable play.",
        spec: &["dsl 0.6.1 §4"],
    },
    Code {
        code: "W-WHEN-TEST-LITERAL",
        summary: r#"A `<when test="…">` arm is written as a CEL literal comparison that the `is=` pattern form would say more clearly and that the checker can reason about directly."#,
        spec: &["dsl 0.18.0 §3", "dsl §7.3.1"],
    },
    Code {
        code: "W-WIP",
        summary: "Under `check-project --wip`, a guard or objective is dead only because a relation it needs has no producer written yet (no seed, `::assert`, rule, or reserved declaration, or only a component `::assert` with an unbound `@param`); the message names the error code it is without `--wip`: `E-ARM-DEAD`, `E-BEAT-UNREACHABLE`, `E-ENTRY-UNREACHABLE`, or `E-OBJECTIVE-UNSATISFIABLE`.",
        spec: &["dsl 0.23.0 §10", "dsl 0.26.0 §2.6"],
    },
];

