---
title: Reserved names
description: "The names Lute keeps for itself, where each is refused, and what to write instead."
---

Some words already mean something in Lute: a state root such as `run`, the
no-value word `unset`, a CEL literal or keyword, a play-script step. A name
you declare cannot be one of them where it would be read as that word — an
entity member called `clock` would be read as the `clock` state root in
`holds('found', ['clock'])`, and a choice called `true` as the boolean in
`<when is="true">`.

So the checker refuses a reserved name where it is declared, not where it is
used. The error is `E-RESERVED-NAME` (`E-PLUGIN-RESERVED-NAME` for a name a
plugin exports, `E-TEMPLATE` for a template param). The message says what the
word already means and names one to use instead, such as `theClock` for the
member or `yes` for the choice. `lute --explain E-RESERVED-NAME` prints this
table in the terminal.

## The table

Each row lists the names, the declarations that cannot take them, what the
names already mean, and the code that reports them.

<!-- reserved-names:begin -->
| Names | Refused as | Because it is | Code |
| --- | --- | --- | --- |
| `unset` | entity member; enum member; scene, beat, entry or choice id | the no-value word: `is="unset"` and `== 'unset'` test for a path that holds nothing, so a value named `unset` can never be matched | E-RESERVED-NAME |
| `true` `false` `null` | entity member; enum member; scene, beat, entry or choice id; relation; season; state path segment (and quest, objective, entry, branch or hub id) | a CEL literal: in a condition and in `is=` it is read as the value, never as a name | E-RESERVED-NAME |
| `_` | entity member; enum member | the wildcard of fact patterns (`holds('knows', ['_'])` in a condition, `knows(_)` in a rule) and the fallback key of `per:` defaults | E-RESERVED-NAME |
| `none` | scene, beat, entry or choice id | the play and test word for no pick and no winner (`pick: none`, `winner: none`) | E-RESERVED-NAME |
| `scene` `run` `user` `app` `quest` `entry` `prev` `clock` `occasion` `season` | entity member; def | a state root: in a condition a bare root name starts a state path, so `@clock` and a bare `clock` read state instead | E-RESERVED-NAME |
| `scene` `run` `user` `app` `quest` `entry` `prev` `clock` `occasion` `season` `day` `week` `slot` | season | a state root or a `once`/tier period, so `once="season:run"` would sit beside `once="run"` meaning something else | E-RESERVED-NAME |
| `as` `break` `const` `continue` `else` `for` `function` `if` `import` `in` `let` `loop` `namespace` `package` `return` `var` `void` `while` | entity member; relation; season; state path segment (and quest, objective, entry, branch or hub id) | a CEL keyword, which a condition cannot write as a name (`quest.in.state` does not parse) | E-RESERVED-NAME |
| `all` `count` `countDistinct` `exists` `exists_one` `filter` `has` `holds` `map` `now` `validAt` `visited` | relation | a CEL host function or macro, or a Datalog aggregate, so a relation of that name would read as the call in a rule body (`count(<name>(…))`) | E-RESERVED-NAME |
| `completed` `active` | relation | an `after:` call (`completed("<quest>")`, `active("<quest>")`), so `after="completed(dorm)"` would read the quest call | E-RESERVED-NAME |
| `cel` `not` | relation | a rule word: in `rules:` `not(…)` negates and `cel("…")` is a condition | E-RESERVED-NAME |
| `narrator` | cast id | the built-in narration speaker: `@narrator:` lines are narration, so a cast entry for it is never shown and its `present:` guards every narrated line | E-RESERVED-NAME, E-PLUGIN-RESERVED-NAME |
| `questActive` `questComplete` `questFailed` | plugin occasion; plugin event | an engine lifecycle event (`<on event="questComplete">`) | E-PLUGIN-RESERVED-NAME |
| `occasion` `newRun` `engine` `event` `advance` `end` | plugin occasion | a play-script step key: `- newRun: true` starts a new run and `- end: true` ends the play, neither raises an occasion of that name | E-PLUGIN-RESERVED-NAME |
| `set` `assert` `retract` `accept` `use` `body` `cut` | plugin directive | a core statement (`::set{…}`, `::use{component=…}`), which content always reads as the core one | E-PLUGIN-RESERVED-NAME |
| `on` `quest` `objective` `match` `branch` `hub` `choice` `when` `otherwise` `entry` `beat` `timeline` `track` `reward` `return` | plugin directive | a core block tag (`<match>`, `<quest>`), which content always reads as the core one | E-PLUGIN-RESERVED-NAME |
| `id` `use` `on` `target` `for` `title` `priority` `once` `share` `after` `when` `spentBy` `also` | beat template param | a beat header key: `<beat use=… when=…>` sets the beat's own `when`, never the param | E-TEMPLATE |
| `component` `when` | component param | a `::use` key of its own (`::use{component=… when=…}`), so the param could never be passed | E-TEMPLATE |
<!-- reserved-names:end -->

## Notes

- A member list holds names. A YAML number (`[1, 2, 3]`), boolean (`true`)
  or `null` in `members:`, `add:` or an enum is refused too. Rename it
  (`floor1`) rather than quoting it.
- `none` is refused only as an id a play or test can name in `pick:` or
  `winner:` (a scene, beat, entry or choice). An enum or entity member may be
  `none` (`weapon: [none, sword]`).
- A season is also refused a `once`/tier period name (`day`, `week`, `slot`),
  because `once="season:week"` would sit beside `once="week"`.
- A kind beat names its kind with a prefix (`target="kind:guest"`). The kind
  itself is declared without it (`entities: { guest: … }`).
