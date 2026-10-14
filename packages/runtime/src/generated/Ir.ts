// THIS FILE IS GENERATED — DO NOT EDIT.
// Generator: packages/runtime/scripts/contract-codegen.ts
// Source: schemas/lute-ir-0.44.schema.json

import * as Schema from "effect/Schema";

/**
 * §4.3 injection provenance (lute_check::inject::Provenance): why a compiler-injected record exists. Present only on injected records; its presence is the injection marker (IR 0.37.0 drops the constant `injected` flag).
 */
export const provenance = Schema.Struct({
  "by": Schema.String,
  "explanation": Schema.String
});
export type provenance = typeof provenance.Type;

/**
 * §4.3 component source stamp on component-expanded records (D8).
 */
export const source = Schema.Struct({
  "component": Schema.String
});
export type source = typeof source.Type;

/**
 * a resolved record position (dsl 0.37.0 §5.3), unique within the artifact; its first segment is the one-based section number. Build-local execution ordering only — never an identity. Fields that carry a position: every command `position`; `choice.converge`/`options[].target`; `hub.converge`/`return`/`options[].target`; `match.converge`/`otherwise`/`arms[].target`; `jump.target`; `quest.objectives[].body`; `on.body`; `entry.body`; `beat.body`.
 */
export const position = Schema.String;
export type position = typeof position.Type;

/**
 * dsl 0.37.0 §5.1: the normative grouping of a record's `kind`, read by runtime dispatch before `kind`. content: line; staging: bg, music, sfx, vfx, actor, camera, cg, video, sequence; state: set, assert, retract; control: choice, match, hub, jump, end, barrier; declaration: quest, on, entry, accept, beat; plugin: plugin.
 */
export const family = Schema.Literals([
  "content",
  "staging",
  "state",
  "control",
  "declaration",
  "plugin"
]);
export type family = typeof family.Type;

/**
 * dsl 0.37.0 §5.2: a record's resolved timing, replacing the flattened 0.36 stamp fields. Omitted when every member is absent; each member is omitted when absent. A runtime reads timing only from here.
 */
export const timing = Schema.Struct({
  "wait": Schema.optionalKey(Schema.Boolean),
  "duration": Schema.optionalKey(Schema.Number
  .pipe(
    Schema.check(Schema.isGreaterThanOrEqualTo(0))
  )),
  "delay": Schema.optionalKey(Schema.Number
  .pipe(
    Schema.check(Schema.isGreaterThanOrEqualTo(0))
  )),
  "at": Schema.optionalKey(Schema.Number
  .pipe(
    Schema.check(Schema.isGreaterThanOrEqualTo(0))
  )),
  "timeline": Schema.optionalKey(Schema.Int
  .pipe(
    Schema.check(Schema.isGreaterThanOrEqualTo(0))
  ))
});
export type timing = typeof timing.Type;

/**
 * dsl 0.37.0 §3.6: one presentation segment of a modified line — a coalesced text run or a pause leaf.
 */
export const segment = Schema.Union([
  Schema.Struct({
    "text": Schema.String,
    "styles": Schema.optionalKey(Schema.Array(Schema.String)),
    "rate": Schema.optionalKey(Schema.Number)
  }),
  Schema.Struct({
    "pause": Schema.Number
    .pipe(
      Schema.check(Schema.isGreaterThanOrEqualTo(0))
    )
  })
]);
export type segment = typeof segment.Type;

export const segments = Schema.Array(Schema.suspend(() => segment));
export type segments = typeof segments.Type;

/**
 * Portable CEL expression AST (dsl 0.32.0 §7, expr.rs). Serialized as an UNTAGGED serde enum — each variant is a bare object distinguished by its keys. A plain JSON walker evaluates it; the parallel `cel` string field carries the standard CEL expression.
 */
export const exprNode = Schema.Union([
  Schema.Struct({
    "int": Schema.Int
    .pipe(
      Schema.check(Schema.isGreaterThanOrEqualTo(-9007199254740992))
    )
  }),
  Schema.Struct({
    "double": Schema.Number
  }),
  Schema.Struct({
    "bool": Schema.Boolean
  }),
  Schema.Struct({
    "string": Schema.String
  }),
  Schema.Struct({
    "list": Schema.Array(Schema.Json)
  }),
  Schema.Struct({
    "path": Schema.String
  }),
  Schema.Struct({
    "index": Schema.Json,
    "key": Schema.Json
  }),
  Schema.Struct({
    "has": Schema.String
  }),
  Schema.Struct({
    "op": Schema.Literals([
      "!",
      "-"
    ]),
    "l": Schema.Json
  }),
  Schema.Struct({
    "op": Schema.Literals([
      "&&",
      "||",
      "==",
      "!=",
      "<",
      "<=",
      ">",
      ">=",
      "+",
      "-",
      "*",
      "/",
      "%",
      "in"
    ]),
    "l": Schema.Json,
    "r": Schema.Json
  }),
  Schema.Struct({
    "cond": Schema.Json,
    "then": Schema.Json,
    "else": Schema.Json
  }),
  Schema.Struct({
    "call": Schema.String,
    "args": Schema.Array(Schema.Json)
  }),
  Schema.Struct({
    "param": Schema.String
  })
]);
export type exprNode = typeof exprNode.Type;

/**
 * dsl 0.32.0 §7: CEL slot object carrying standard CEL expression text, lowered portable expr AST, and optional authored source text.
 */
export const celSlot = Schema.Struct({
  "cel": Schema.String,
  "expr": Schema.Json,
  "authored": Schema.optionalKey(Schema.String)
});
export type celSlot = typeof celSlot.Type;

/**
 * dsl 0.32.0 §1.4 / §7: the CEL environment (variables and host function signatures) expressions were checked against.
 */
export const celEnv = Schema.Struct({
  "variables": Schema.Array(Schema.Struct({
    "name": Schema.String,
    "type": Schema.String
  })),
  "functions": Schema.Array(Schema.Struct({
    "name": Schema.String,
    "overloads": Schema.Array(Schema.Struct({
      "params": Schema.Array(Schema.String),
      "result": Schema.String
    }))
  }))
});
export type celEnv = typeof celEnv.Type;

/**
 * One {{…}} interpolation placeholder (IR A3), internally tagged on `kind` (lowercase). The runtime substitutes it against live state; the enclosing `text` keeps the verbatim {{…}} marker.
 */
export const placeholder = Schema.Union([
  Schema.Struct({
    "kind": Schema.Literal("path"),
    "path": Schema.String,
    "format": Schema.optionalKey(Schema.suspend(() => placeholderFormat)),
    "forms": Schema.optionalKey(Schema.suspend(() => placeholderForms))
  }),
  Schema.Struct({
    "kind": Schema.Literal("ref"),
    "ref": Schema.String,
    "expr": Schema.optionalKey(Schema.suspend(() => celSlot)),
    "format": Schema.optionalKey(Schema.suspend(() => placeholderFormat)),
    "forms": Schema.optionalKey(Schema.suspend(() => placeholderForms))
  }),
  Schema.Struct({
    "kind": Schema.Literal("reserved"),
    "token": Schema.String,
    "format": Schema.optionalKey(Schema.suspend(() => placeholderFormat))
  }),
  Schema.Struct({
    "kind": Schema.Literal("occasionTarget"),
    "entityKind": Schema.String,
    "format": Schema.optionalKey(Schema.suspend(() => placeholderFormat))
  })
])
.pipe(
  Schema.toTaggedUnion("kind")
);
export type placeholder = typeof placeholder.Type;

/**
 * dsl 0.24.0 §4 / 0.25.0 §8 / 0.27.0 §7: the interpolation's format hint ({{user.deaths:ordinal}}), omitted when none was written. The engine renders the value in that format (a locale-aware engine localizes it). `ordinal`: a non-negative integer as an English ordinal (1st 2nd 3rd 4th … 11th 12th 13th … 21st 22nd … 101st 111th). `ordinalWord`: an ordinal word (first second … twentieth for 1–20, the `ordinal` digits above). `cardinalWord`: a cardinal word (one two … twenty for 1–20, the digits otherwise). `plural`: the placeholder's `forms` — the first when the number is 1, the second otherwise, with `#word` replaced by the number as a cardinal word, `#Word` by that word capitalized and any other `#` by the number. The text hints apply to the text the value renders as (a label, a cast name, a string): `capitalize` upper-cases its first letter; `start` renders the member's declared `labelForms` `start` form, else `capitalize`; `indefinite` renders the declared `indefinite` form, else `an` (before a vowel) or `a`, a space and the text. Any other value renders unchanged.
 */
export const placeholderFormat = Schema.Literals([
  "ordinal",
  "ordinalWord",
  "cardinalWord",
  "plural",
  "capitalize",
  "start",
  "indefinite"
]);
export type placeholderFormat = typeof placeholderFormat.Type;

/**
 * member → the label forms a kind's `labels:` entry declares beside its text (`{ text, start, indefinite }`): `start` is what a `:start` placeholder renders, `indefinite` what an `:indefinite` one renders. Omitted when no member declares a form; an absent form falls back (see `placeholderFormat`).
 */
export const labelForms = Schema.Record(Schema.String, Schema.Struct({
  "start": Schema.optionalKey(Schema.String),
  "indefinite": Schema.optionalKey(Schema.String)
}));
export type labelForms = typeof labelForms.Type;

/**
 * dsl 0.27.0 §7: a `plural` placeholder's forms as written in {{n:plural(lantern|lanterns)}} — the English singular, then the plural; a `#` in a form stands for the number. Present exactly when `format` is `plural`; an engine may localize the count from them.
 */
export const placeholderForms = Schema.Array(Schema.String);
export type placeholderForms = typeof placeholderForms.Type;

/**
 * dsl 0.37.0 D6: the line's authored delivery role. Every role carries a `voiceKey` — a key is a join, not a recording obligation.
 */
export const role = Schema.Literals([
  "dialogue",
  "narration",
  "mono",
  "os",
  "vo"
]);
export type role = typeof role.Type;

/**
 * One merged relational entity kind (dsl 0.3.0 §3.1).
 */
export const entityKindEntry = Schema.Struct({
  "name": Schema.String,
  "members": Schema.optionalKey(Schema.Array(Schema.String)),
  "open": Schema.Boolean,
  "labels": Schema.optionalKey(Schema.Record(Schema.String, Schema.String)),
  "labelForms": Schema.optionalKey(Schema.suspend(() => labelForms))
});
export type entityKindEntry = typeof entityKindEntry.Type;

/**
 * One merged relational `enums:` entry (dsl 0.3.0 §3).
 */
export const enumEntry = Schema.Struct({
  "name": Schema.String,
  "members": Schema.Array(Schema.String),
  "labels": Schema.optionalKey(Schema.Record(Schema.String, Schema.String))
});
export type enumEntry = typeof enumEntry.Type;

/**
 * One merged relation declaration (dsl 0.3.0 §4).
 */
export const relationEntry = Schema.Struct({
  "name": Schema.String,
  "args": Schema.Array(Schema.String),
  "tier": Schema.optionalKey(Schema.String),
  "derive": Schema.Boolean,
  "reserved": Schema.Boolean,
  "key": Schema.optionalKey(Schema.Array(Schema.Int
  .pipe(
    Schema.check(Schema.isGreaterThanOrEqualTo(0))
  ))),
  "excludes": Schema.optionalKey(Schema.Array(Schema.String))
});
export type relationEntry = typeof relationEntry.Type;

/**
 * One seed `facts:` ground tuple (dsl 0.3.0 §4).
 */
export const seedFactEntry = Schema.Struct({
  "relation": Schema.String,
  "args": Schema.Array(Schema.String)
});
export type seedFactEntry = typeof seedFactEntry.Type;

/**
 * One rule term (dsl 0.3.0 §7.1), tagged on `kind`. Bools lower to a Const with value "true"/"false".
 */
export const termEntry = Schema.Union([
  Schema.Struct({
    "kind": Schema.Literal("var"),
    "name": Schema.String
  }),
  Schema.Struct({
    "kind": Schema.Literal("const"),
    "value": Schema.String
  })
])
.pipe(
  Schema.toTaggedUnion("kind")
);
export type termEntry = typeof termEntry.Type;

/**
 * One rule atom: a relation applied to terms (dsl 0.3.0 §7.1).
 */
export const atomEntry = Schema.Struct({
  "relation": Schema.String,
  "terms": Schema.Array(Schema.suspend(() => termEntry))
});
export type atomEntry = typeof atomEntry.Type;

/**
 * One rule body literal (dsl 0.3.0 §7.1), tagged on `kind`: a positive/negated atom, a CEL guard, or a term comparison.
 */
export const bodyEntry = Schema.Union([
  Schema.Struct({
    "kind": Schema.Literal("atom"),
    "atom": Schema.suspend(() => atomEntry),
    "negated": Schema.Boolean
  }),
  Schema.Struct({
    "kind": Schema.Literal("guard"),
    "cel": Schema.suspend(() => celSlot)
  }),
  Schema.Struct({
    "kind": Schema.Literal("cmp"),
    "lhs": Schema.suspend(() => termEntry),
    "rhs": Schema.suspend(() => termEntry),
    "negated": Schema.Boolean
  }),
  Schema.Struct({
    "kind": Schema.Literal("count"),
    "atom": Schema.suspend(() => atomEntry),
    "distinct": Schema.optionalKey(Schema.Array(Schema.String)),
    "op": Schema.Literals([
      "==",
      "!=",
      "<",
      "<=",
      ">",
      ">="
    ]),
    "n": Schema.Int
    .pipe(
      Schema.check(Schema.isGreaterThanOrEqualTo(0))
    )
  })
])
.pipe(
  Schema.toTaggedUnion("kind")
);
export type bodyEntry = typeof bodyEntry.Type;

/**
 * One Datalog rule (dsl 0.3.0 §7.1), emitted as STRUCTURED data. Lute performs NO evaluation (D1); the engine runs the least-fixpoint over this rule set.
 */
export const ruleEntry = Schema.Struct({
  "head": Schema.suspend(() => atomEntry),
  "body": Schema.Array(Schema.suspend(() => bodyEntry)),
  "raw": Schema.String
});
export type ruleEntry = typeof ruleEntry.Type;

/**
 * One advisory graph edge (connectivity spec §2.6, T13): a single node's raw declared `after` (scene, bundle beat) or `follows` (quest), verbatim.
 */
export const prereqEdgeEntry = Schema.Struct({
  "node": Schema.String,
  "after": Schema.optionalKey(Schema.String),
  "follows": Schema.optionalKey(Schema.String),
  "formula": Schema.optionalKey(Schema.Json)
}).check(Schema.makeFilter((value) => [
  [
    "after"
  ],
  [
    "follows"
  ]
].filter((keys) => keys.every((key) => key in value)).length === 1 || "expected exactly one of the key sets: after | follows"));
export type prereqEdgeEntry = typeof prereqEdgeEntry.Type;

/**
 * A parsed `after` prerequisite formula: `visited("id")` / `completed("q")` / `active("q")` atoms joined by `&&` (`and`) / `||` (`or`), each binary node a two-element array left, right.
 */
export const prereqFormula = Schema.Union([
  Schema.Struct({
    "visited": Schema.String
  }),
  Schema.Struct({
    "completed": Schema.String
  }),
  Schema.Struct({
    "active": Schema.String
  }),
  Schema.Struct({
    "and": Schema.Tuple([
      Schema.Json,
      Schema.Json
    ])
  }),
  Schema.Struct({
    "or": Schema.Tuple([
      Schema.Json,
      Schema.Json
    ])
  })
]);
export type prereqFormula = typeof prereqFormula.Type;

/**
 * Scene-kind envelope meta (dsl 0.15.0 §2/§3). Present when `kind` == "scene". Since 0.15.0 only `id` is required — the four legacy identity keys demote to optional.
 */
export const sceneMeta = Schema.Struct({
  "id": Schema.String,
  "character": Schema.optionalKey(Schema.String),
  "season": Schema.optionalKey(Schema.Int),
  "episode": Schema.optionalKey(Schema.Int),
  "episodeId": Schema.optionalKey(Schema.String),
  "title": Schema.optionalKey(Schema.String),
  "extra": Schema.optionalKey(Schema.Record(Schema.String, Schema.Union([
    Schema.String,
    Schema.Int,
    Schema.Number,
    Schema.Boolean,
    Schema.Array(Schema.Union([
      Schema.String,
      Schema.Int,
      Schema.Number,
      Schema.Boolean
    ]))
  ]))),
  "plugin": Schema.optionalKey(Schema.Json),
  "beat": Schema.optionalKey(Schema.suspend(() => sceneBeat))
});
export type sceneMeta = typeof sceneMeta.Type;

/**
 * dsl 0.26.0 §5: a `target: "kind:<kind>"` beat, resolved. It answers a raise for `<prefix>.<member>` of every listed member; while it runs, `occasion.target` is the raised member id.
 */
export const targetKind = Schema.Struct({
  "kind": Schema.String,
  "prefix": Schema.String,
  "members": Schema.Array(Schema.String)
});
export type targetKind = typeof targetKind.Type;

/**
 * dsl 0.27.0 §3 (T2-10): a `for: "kind:<kind>"` beat on an untargeted `select: sequence` occasion, resolved. Each raise presents the beat once per listed member whose `when` holds (judged at the raise with `occasion.target` bound to that member), in list order; while one presentation runs, `occasion.target` is that member id. `once` spends the beat, not a member.
 */
export const forKind = Schema.Struct({
  "kind": Schema.String,
  "members": Schema.Array(Schema.String)
});
export type forKind = typeof forKind.Type;

/**
 * A scene beat (dsl 0.21.0 §3.1; docs/runtime/beats-and-occasions.md): the occasion the scene answers and how it competes with the other candidates when the engine raises it.
 */
export const sceneBeat = Schema.Struct({
  "on": Schema.String
  .pipe(
    Schema.check(Schema.isPattern(new RegExp("^[A-Za-z0-9_][A-Za-z0-9_-]*$")))
  ),
  "targetKind": Schema.optionalKey(Schema.suspend(() => targetKind)),
  "forKind": Schema.optionalKey(Schema.suspend(() => forKind)),
  "target": Schema.optionalKey(Schema.String),
  "when": Schema.optionalKey(Schema.suspend(() => celSlot)),
  "priority": Schema.Int,
  "once": Schema.Union([
    Schema.Literals([
      "run",
      "user",
      "none",
      "day",
      "slot",
      "week"
    ]),
    Schema.String
    .pipe(
      Schema.check(Schema.isPattern(new RegExp("^season:[A-Za-z0-9_][A-Za-z0-9_-]*$")))
    )
  ]),
  "also": Schema.optionalKey(Schema.Literal(true)),
  "spentBy": Schema.optionalKey(Schema.suspend(() => celSlot)),
  "share": Schema.optionalKey(Schema.String
  .pipe(
    Schema.check(Schema.isPattern(new RegExp("^[A-Za-z0-9_][A-Za-z0-9_-]*$")))
  )),
  "advances": Schema.optionalKey(Schema.Union([
    Schema.Literals([
      "slot",
      "day"
    ]),
    Schema.Int
    .pipe(
      Schema.check(Schema.isGreaterThanOrEqualTo(1))
    )
  ]))
});
export type sceneBeat = typeof sceneBeat.Type;

/**
 * Quest-kind envelope meta (dsl 0.2.0 §6.1, dsl 0.15.0 §3, dsl 0.19.0 §2.1). MAY serialize as {} when none of id/title/contentLang/extra/plugin are authored. Present when `kind` == "quest".
 */
export const questMeta = Schema.Struct({
  "id": Schema.optionalKey(Schema.String
  .pipe(
    Schema.check(Schema.isPattern(new RegExp("^[A-Za-z0-9_][A-Za-z0-9_-]*(\\.[A-Za-z0-9_][A-Za-z0-9_-]*)*$")))
  )),
  "title": Schema.optionalKey(Schema.String),
  "contentLang": Schema.optionalKey(Schema.String),
  "extra": Schema.optionalKey(Schema.Record(Schema.String, Schema.Union([
    Schema.String,
    Schema.Int,
    Schema.Number,
    Schema.Boolean,
    Schema.Array(Schema.Union([
      Schema.String,
      Schema.Int,
      Schema.Number,
      Schema.Boolean
    ]))
  ]))),
  "plugin": Schema.optionalKey(Schema.Json)
});
export type questMeta = typeof questMeta.Type;

/**
 * One folded state slot (§4.1): the engine's init/type table entry.
 */
export const stateEntry = Schema.Struct({
  "path": Schema.String,
  "type": Schema.String,
  "domain": Schema.optionalKey(Schema.Array(Schema.String)),
  "default": Schema.optionalKey(Schema.Json),
  "owner": Schema.optionalKey(Schema.Literals([
    "engine"
  ])),
  "provenance": Schema.optionalKey(Schema.String),
  "labels": Schema.optionalKey(Schema.Record(Schema.String, Schema.String)),
  "labelForms": Schema.optionalKey(Schema.suspend(() => labelForms))
});
export type stateEntry = typeof stateEntry.Type;

/**
 * The origin of an Effect's value (IR A12, untagged).
 */
export const effectSource = Schema.Union([
  Schema.Struct({
    "bridgeResult": Schema.String
  }),
  Schema.Struct({
    "op": Schema.String,
    "by": Schema.Number
  }),
  Schema.Union([
    Schema.Boolean,
    Schema.Number,
    Schema.String
  ])
]);
export type effectSource = typeof effectSource.Type;

/**
 * A resolved plugin state-write binding (IR A12): where a bridge result / increment / literal lands. The runtime applies it after the bridge call — no manifest lookup, no per-plugin knowledge.
 */
export const effect = Schema.Struct({
  "path": Schema.String,
  "from": Schema.suspend(() => effectSource)
});
export type effect = typeof effect.Type;

/**
 * One <choice> option.
 */
export const choiceOption = Schema.Struct({
  "id": Schema.String,
  "text": Schema.String,
  "lineId": Schema.String,
  "when": Schema.optionalKey(Schema.suspend(() => celSlot)),
  "target": Schema.suspend(() => position),
  "placeholders": Schema.optionalKey(Schema.Array(Schema.suspend(() => placeholder))),
  "texts": Schema.optionalKey(Schema.Record(Schema.String, Schema.String))
});
export type choiceOption = typeof choiceOption.Type;

/**
 * One <hub> option: a choice option plus always-present once/exit revisit flags.
 */
export const hubOption = Schema.Struct({
  "id": Schema.String,
  "text": Schema.String,
  "lineId": Schema.String,
  "once": Schema.Boolean,
  "exit": Schema.Boolean,
  "when": Schema.optionalKey(Schema.suspend(() => celSlot)),
  "target": Schema.suspend(() => position),
  "placeholders": Schema.optionalKey(Schema.Array(Schema.suspend(() => placeholder))),
  "texts": Schema.optionalKey(Schema.Record(Schema.String, Schema.String))
});
export type hubOption = typeof hubOption.Type;

/**
 * One <match> arm.
 */
export const matchArm = Schema.Struct({
  "is": Schema.optionalKey(Schema.String),
  "test": Schema.suspend(() => celSlot),
  "target": Schema.suspend(() => position)
});
export type matchArm = typeof matchArm.Type;

/**
 * One `<reward id= kind= target= amount= when= outcome=/>` declaration (dsl 0.16.0 §2/§3). Pure data — the compiler NEVER synthesizes handler bodies or `Command::Set` records for a reward; the reference engine (`crates/lute-cli/src/runner.rs` `Runner::emit_grants`) emits deterministic `grant` transcript events at each fresh lifecycle transition, and `lute trace` mirrors those events (`crates/lute-trace/src/walk.rs`).
 */
export const rewardEntry = Schema.Struct({
  "id": Schema.optionalKey(Schema.String
  .pipe(
    Schema.check(Schema.isPattern(new RegExp("^[A-Za-z][A-Za-z0-9_-]{0,63}$")))
  )),
  "kind": Schema.String,
  "target": Schema.optionalKey(Schema.String),
  "amount": Schema.optionalKey(Schema.Int),
  "amountMin": Schema.optionalKey(Schema.Int),
  "amountMax": Schema.optionalKey(Schema.Int),
  "when": Schema.optionalKey(Schema.suspend(() => celSlot)),
  "outcome": Schema.optionalKey(Schema.Literals([
    "failed"
  ])),
  "credits": Schema.optionalKey(Schema.String)
});
export type rewardEntry = typeof rewardEntry.Type;

/**
 * One objective inlined in QuestCmd.objectives (dsl 0.2.0 §6.4). Declaration data only; the engine derives completion (all non-optional objectives done ⇒ quest complete).
 */
export const objectiveEntry = Schema.Struct({
  "id": Schema.String,
  "title": Schema.optionalKey(Schema.String),
  "titleLineId": Schema.optionalKey(Schema.String),
  "done": Schema.suspend(() => celSlot),
  "visibleWhen": Schema.optionalKey(Schema.suspend(() => celSlot)),
  "optional": Schema.Boolean,
  "body": Schema.NullOr(Schema.suspend(() => position)),
  "quest": Schema.optionalKey(Schema.String),
  "rewards": Schema.optionalKey(Schema.Array(Schema.suspend(() => rewardEntry))),
  "on": Schema.optionalKey(Schema.String
  .pipe(
    Schema.check(Schema.isPattern(new RegExp("^[A-Za-z0-9_][A-Za-z0-9_-]*$")))
  )),
  "by": Schema.optionalKey(Schema.suspend(() => celSlot)),
  "target": Schema.optionalKey(Schema.String),
  "until": Schema.optionalKey(Schema.suspend(() => celSlot))
});
export type objectiveEntry = typeof objectiveEntry.Type;

/**
 * A spoken or narrated content line (dsl 0.37.0 §5.3).
 */
export const cmdLine = Schema.Struct({
  "kind": Schema.Literal("line"),
  "family": Schema.Literal("content"),
  "position": Schema.suspend(() => position),
  "role": Schema.suspend(() => role),
  "speaker": Schema.String,
  "text": Schema.String,
  "emotion": Schema.optionalKey(Schema.String),
  "variant": Schema.optionalKey(Schema.Int),
  "action": Schema.optionalKey(Schema.String),
  "dialogMotion": Schema.optionalKey(Schema.String),
  "as": Schema.optionalKey(Schema.String),
  "lineId": Schema.String,
  "voiceKey": Schema.String,
  "placeholders": Schema.optionalKey(Schema.Array(Schema.suspend(() => placeholder))),
  "texts": Schema.optionalKey(Schema.Record(Schema.String, Schema.String)),
  "segments": Schema.optionalKey(Schema.suspend(() => segments)),
  "localeSegments": Schema.optionalKey(Schema.Record(Schema.String, Schema.suspend(() => segments))),
  "timing": Schema.optionalKey(Schema.suspend(() => timing)),
  "provenance": Schema.optionalKey(Schema.suspend(() => provenance)),
  "source": Schema.optionalKey(Schema.suspend(() => source))
});
export type cmdLine = typeof cmdLine.Type;

/**
 * `::bg` (dsl 0.37.0 §5.3): sets the stage backdrop. Every field optional.
 */
export const cmdBg = Schema.Struct({
  "kind": Schema.Literal("bg"),
  "family": Schema.Literal("staging"),
  "position": Schema.suspend(() => position),
  "location": Schema.optionalKey(Schema.String),
  "time": Schema.optionalKey(Schema.String),
  "assetId": Schema.optionalKey(Schema.String),
  "timing": Schema.optionalKey(Schema.suspend(() => timing)),
  "provenance": Schema.optionalKey(Schema.suspend(() => provenance)),
  "source": Schema.optionalKey(Schema.suspend(() => source))
});
export type cmdBg = typeof cmdBg.Type;

/**
 * `::music` (dsl 0.37.0 §5.3). `playback` is a project `musicPlayback` member.
 */
export const cmdMusic = Schema.Struct({
  "kind": Schema.Literal("music"),
  "family": Schema.Literal("staging"),
  "position": Schema.suspend(() => position),
  "playback": Schema.optionalKey(Schema.String),
  "stop": Schema.optionalKey(Schema.Literal(true)),
  "mood": Schema.optionalKey(Schema.String),
  "volume": Schema.optionalKey(Schema.String),
  "assetId": Schema.optionalKey(Schema.String),
  "timing": Schema.optionalKey(Schema.suspend(() => timing)),
  "provenance": Schema.optionalKey(Schema.suspend(() => provenance)),
  "source": Schema.optionalKey(Schema.suspend(() => source))
});
export type cmdMusic = typeof cmdMusic.Type;

/**
 * `::sfx` (dsl 0.37.0 §5.3).
 */
export const cmdSfx = Schema.Struct({
  "kind": Schema.Literal("sfx"),
  "family": Schema.Literal("staging"),
  "position": Schema.suspend(() => position),
  "sound": Schema.optionalKey(Schema.String),
  "assetId": Schema.optionalKey(Schema.String),
  "timing": Schema.optionalKey(Schema.suspend(() => timing)),
  "provenance": Schema.optionalKey(Schema.suspend(() => provenance)),
  "source": Schema.optionalKey(Schema.suspend(() => source))
});
export type cmdSfx = typeof cmdSfx.Type;

/**
 * `::vfx` (dsl 0.37.0 §5.3).
 */
export const cmdVfx = Schema.Struct({
  "kind": Schema.Literal("vfx"),
  "family": Schema.Literal("staging"),
  "position": Schema.suspend(() => position),
  "type": Schema.String,
  "label": Schema.optionalKey(Schema.String),
  "transition": Schema.optionalKey(Schema.String),
  "timing": Schema.optionalKey(Schema.suspend(() => timing)),
  "provenance": Schema.optionalKey(Schema.suspend(() => provenance)),
  "source": Schema.optionalKey(Schema.suspend(() => source))
});
export type cmdVfx = typeof cmdVfx.Type;

/**
 * `::actor` (dsl 0.37.0 §5.1/§5.3): an authored actor record or a compiler-injected one (anchor, preload, position reset, hide — §7.4). Injected records carry `provenance`; `posReset` and `preload` appear only on injected records.
 */
export const cmdActor = Schema.Struct({
  "kind": Schema.Literal("actor"),
  "family": Schema.Literal("staging"),
  "position": Schema.suspend(() => position),
  "character": Schema.String,
  "anchor": Schema.optionalKey(Schema.String),
  "action": Schema.optionalKey(Schema.String),
  "exit": Schema.optionalKey(Schema.Boolean),
  "emotion": Schema.optionalKey(Schema.String),
  "costume": Schema.optionalKey(Schema.String),
  "posReset": Schema.optionalKey(Schema.Boolean),
  "preload": Schema.optionalKey(Schema.Boolean),
  "timing": Schema.optionalKey(Schema.suspend(() => timing)),
  "provenance": Schema.optionalKey(Schema.suspend(() => provenance)),
  "source": Schema.optionalKey(Schema.suspend(() => source))
});
export type cmdActor = typeof cmdActor.Type;

/**
 * `::camera` (dsl 0.37.0 D1, §5.3): opaque project-domain members; at least one is present after checking. No numeric transform is synthesized.
 */
export const cmdCamera = Schema.Struct({
  "kind": Schema.Literal("camera"),
  "family": Schema.Literal("staging"),
  "position": Schema.suspend(() => position),
  "focus": Schema.optionalKey(Schema.String),
  "framing": Schema.optionalKey(Schema.String),
  "move": Schema.optionalKey(Schema.String),
  "transition": Schema.optionalKey(Schema.String),
  "timing": Schema.optionalKey(Schema.suspend(() => timing)),
  "provenance": Schema.optionalKey(Schema.suspend(() => provenance)),
  "source": Schema.optionalKey(Schema.suspend(() => source))
}).check(Schema.makeFilter((value) => [
  [
    "focus"
  ],
  [
    "framing"
  ],
  [
    "move"
  ],
  [
    "transition"
  ]
].filter((keys) => keys.every((key) => key in value)).length >= 1 || "expected at least one of the key sets: focus | framing | move | transition"));
export type cmdCamera = typeof cmdCamera.Type;

/**
 * `::cg` (dsl 0.37.0 D2, §5.3): a CG image.
 */
export const cmdCg = Schema.Struct({
  "kind": Schema.Literal("cg"),
  "family": Schema.Literal("staging"),
  "position": Schema.suspend(() => position),
  "assetId": Schema.String,
  "display": Schema.String,
  "layout": Schema.optionalKey(Schema.String),
  "timing": Schema.optionalKey(Schema.suspend(() => timing)),
  "provenance": Schema.optionalKey(Schema.suspend(() => provenance)),
  "source": Schema.optionalKey(Schema.suspend(() => source))
});
export type cmdCg = typeof cmdCg.Type;

/**
 * `::video` (dsl 0.37.0 D2, §5.3).
 */
export const cmdVideo = Schema.Struct({
  "kind": Schema.Literal("video"),
  "family": Schema.Literal("staging"),
  "position": Schema.suspend(() => position),
  "assetId": Schema.String,
  "display": Schema.String,
  "timing": Schema.optionalKey(Schema.suspend(() => timing)),
  "provenance": Schema.optionalKey(Schema.suspend(() => provenance)),
  "source": Schema.optionalKey(Schema.suspend(() => source))
});
export type cmdVideo = typeof cmdVideo.Type;

/**
 * `::sequence{name}` (dsl 0.37.0 D3, §5.3): a reference to a project-declared external cinematic. Carries only its name and resolved `timing` (`wait` defaults to true); no bound characters. The reference runner records it without simulating the sequence.
 */
export const cmdSequence = Schema.Struct({
  "kind": Schema.Literal("sequence"),
  "family": Schema.Literal("staging"),
  "position": Schema.suspend(() => position),
  "name": Schema.String,
  "timing": Schema.optionalKey(Schema.suspend(() => timing)),
  "provenance": Schema.optionalKey(Schema.suspend(() => provenance)),
  "source": Schema.optionalKey(Schema.suspend(() => source))
});
export type cmdSequence = typeof cmdSequence.Type;

/**
 * A state write (§4.4).
 */
export const cmdSet = Schema.Struct({
  "kind": Schema.Literal("set"),
  "family": Schema.Literal("state"),
  "position": Schema.suspend(() => position),
  "path": Schema.String,
  "op": Schema.String,
  "value": Schema.suspend(() => celSlot),
  "timing": Schema.optionalKey(Schema.suspend(() => timing)),
  "provenance": Schema.optionalKey(Schema.suspend(() => provenance)),
  "source": Schema.optionalKey(Schema.suspend(() => source))
});
export type cmdSet = typeof cmdSet.Type;

/**
 * One asserted fact delta (dsl 0.3.0 §5). DATA only — the engine applies it as a positive write to the relation's fact set.
 */
export const cmdAssert = Schema.Struct({
  "kind": Schema.Literal("assert"),
  "family": Schema.Literal("state"),
  "position": Schema.suspend(() => position),
  "relation": Schema.String,
  "args": Schema.Array(Schema.String),
  "timing": Schema.optionalKey(Schema.suspend(() => timing)),
  "provenance": Schema.optionalKey(Schema.suspend(() => provenance)),
  "source": Schema.optionalKey(Schema.suspend(() => source))
});
export type cmdAssert = typeof cmdAssert.Type;

/**
 * One retracted fact delta (dsl 0.3.0 §5). The engine applies it as a negative write; "_" positions are a bulk wildcard the engine resolves.
 */
export const cmdRetract = Schema.Struct({
  "kind": Schema.Literal("retract"),
  "family": Schema.Literal("state"),
  "position": Schema.suspend(() => position),
  "relation": Schema.String,
  "args": Schema.Array(Schema.String),
  "timing": Schema.optionalKey(Schema.suspend(() => timing)),
  "provenance": Schema.optionalKey(Schema.suspend(() => provenance)),
  "source": Schema.optionalKey(Schema.suspend(() => source))
});
export type cmdRetract = typeof cmdRetract.Type;

/**
 * `<choice>` (dsl 0.37.0 §5.3): a player selection; each option's segment is followed by `converge`.
 */
export const cmdChoice = Schema.Struct({
  "kind": Schema.Literal("choice"),
  "family": Schema.Literal("control"),
  "position": Schema.suspend(() => position),
  "branchId": Schema.String,
  "selectionKey": Schema.String,
  "options": Schema.Array(Schema.suspend(() => choiceOption)),
  "converge": Schema.suspend(() => position),
  "prompt": Schema.optionalKey(Schema.String),
  "timeout": Schema.optionalKey(Schema.Int
  .pipe(
    Schema.check(Schema.isGreaterThanOrEqualTo(1))
  )),
  "default": Schema.optionalKey(Schema.String),
  "timing": Schema.optionalKey(Schema.suspend(() => timing)),
  "provenance": Schema.optionalKey(Schema.suspend(() => provenance)),
  "source": Schema.optionalKey(Schema.suspend(() => source))
});
export type cmdChoice = typeof cmdChoice.Type;

/**
 * `<match>` (dsl 0.37.0 §5.3): the first arm whose test holds runs; otherwise `otherwise`, then `converge`.
 */
export const cmdMatch = Schema.Struct({
  "kind": Schema.Literal("match"),
  "family": Schema.Literal("control"),
  "position": Schema.suspend(() => position),
  "subject": Schema.optionalKey(Schema.suspend(() => celSlot)),
  "arms": Schema.Array(Schema.suspend(() => matchArm)),
  "otherwise": Schema.optionalKey(Schema.suspend(() => position)),
  "converge": Schema.suspend(() => position),
  "timing": Schema.optionalKey(Schema.suspend(() => timing)),
  "provenance": Schema.optionalKey(Schema.suspend(() => provenance)),
  "source": Schema.optionalKey(Schema.suspend(() => source))
});
export type cmdMatch = typeof cmdMatch.Type;

/**
 * <hub> (§7.3.2): a choice plus revisit semantics; re-presentation is a runtime property of the hub kind (no backward jump emitted).
 */
export const cmdHub = Schema.Struct({
  "kind": Schema.Literal("hub"),
  "family": Schema.Literal("control"),
  "position": Schema.suspend(() => position),
  "id": Schema.String,
  "selectionKey": Schema.String,
  "options": Schema.Array(Schema.suspend(() => hubOption)),
  "converge": Schema.suspend(() => position),
  "prompt": Schema.optionalKey(Schema.String),
  "return": Schema.optionalKey(Schema.suspend(() => position)),
  "timing": Schema.optionalKey(Schema.suspend(() => timing)),
  "provenance": Schema.optionalKey(Schema.suspend(() => provenance)),
  "source": Schema.optionalKey(Schema.suspend(() => source))
});
export type cmdHub = typeof cmdHub.Type;

/**
 * Unconditional control-flow jump. Carries no `timing`, `provenance`, or `source`.
 */
export const cmdJump = Schema.Struct({
  "kind": Schema.Literal("jump"),
  "family": Schema.Literal("control"),
  "position": Schema.suspend(() => position),
  "target": Schema.suspend(() => position)
});
export type cmdJump = typeof cmdJump.Type;

/**
 * dsl 0.8.0 §3: terminate the walk at this record. Equivalent to falling off the end of `commands`, except `reason` is carried.
 */
export const cmdEnd = Schema.Struct({
  "kind": Schema.Literal("end"),
  "family": Schema.Literal("control"),
  "position": Schema.suspend(() => position),
  "reason": Schema.optionalKey(Schema.String),
  "timing": Schema.optionalKey(Schema.suspend(() => timing)),
  "provenance": Schema.optionalKey(Schema.suspend(() => provenance)),
  "source": Schema.optionalKey(Schema.suspend(() => source))
});
export type cmdEnd = typeof cmdEnd.Type;

/**
 * A timeline join point (dsl 0.37.0 §5.2). Carries no `timing`: its join fields are direct, so they cannot collide with a record's timing. The reference runner records it as a join and simulates no elapsed time.
 */
export const cmdBarrier = Schema.Struct({
  "kind": Schema.Literal("barrier"),
  "family": Schema.Literal("control"),
  "position": Schema.suspend(() => position),
  "timeline": Schema.Int
  .pipe(
    Schema.check(Schema.isGreaterThanOrEqualTo(0))
  ),
  "at": Schema.Number
  .pipe(
    Schema.check(Schema.isGreaterThanOrEqualTo(0))
  )
});
export type cmdBarrier = typeof cmdBarrier.Type;

/**
 * <quest> declaration head (dsl 0.2.0 §6.3). Objective completion bodies and <on> arms follow as their own positioned records.
 */
export const cmdQuest = Schema.Struct({
  "kind": Schema.Literal("quest"),
  "family": Schema.Literal("declaration"),
  "position": Schema.suspend(() => position),
  "id": Schema.String,
  "title": Schema.optionalKey(Schema.String),
  "titleLineId": Schema.optionalKey(Schema.String),
  "start": Schema.optionalKey(Schema.suspend(() => celSlot)),
  "fail": Schema.optionalKey(Schema.suspend(() => celSlot)),
  "objectives": Schema.Array(Schema.suspend(() => objectiveEntry)),
  "rewards": Schema.optionalKey(Schema.Array(Schema.suspend(() => rewardEntry))),
  "rearm": Schema.optionalKey(Schema.suspend(() => celSlot)),
  "tier": Schema.optionalKey(Schema.Union([
    Schema.Literals([
      "run"
    ]),
    Schema.String
    .pipe(
      Schema.check(Schema.isPattern(new RegExp("^season:[A-Za-z0-9_][A-Za-z0-9_-]*$")))
    )
  ])),
  "activate": Schema.optionalKey(Schema.Literals([
    "accept"
  ])),
  "complete": Schema.optionalKey(Schema.Literals([
    "any"
  ])),
  "accept": Schema.optionalKey(Schema.Literals([
    "external"
  ])),
  "timing": Schema.optionalKey(Schema.suspend(() => timing)),
  "provenance": Schema.optionalKey(Schema.suspend(() => provenance)),
  "source": Schema.optionalKey(Schema.suspend(() => source))
});
export type cmdQuest = typeof cmdQuest.Type;

/**
 * <on> event-condition-action record (dsl 0.2.0 §4, §6.6): an independent event rule.
 */
export const cmdOn = Schema.Struct({
  "kind": Schema.Literal("on"),
  "family": Schema.Literal("declaration"),
  "position": Schema.suspend(() => position),
  "event": Schema.String,
  "when": Schema.optionalKey(Schema.suspend(() => celSlot)),
  "body": Schema.suspend(() => position),
  "target": Schema.optionalKey(Schema.String),
  "timing": Schema.optionalKey(Schema.suspend(() => timing)),
  "provenance": Schema.optionalKey(Schema.suspend(() => provenance)),
  "source": Schema.optionalKey(Schema.suspend(() => source))
});
export type cmdOn = typeof cmdOn.Type;

/**
 * <entry> declaration head of a lore artifact (dsl 0.19.0 §7, docs/runtime/lore-entries.md): content the engine looks up rather than plays.
 */
export const cmdEntry = Schema.Struct({
  "kind": Schema.Literal("entry"),
  "family": Schema.Literal("declaration"),
  "position": Schema.suspend(() => position),
  "id": Schema.String,
  "target": Schema.optionalKey(Schema.String),
  "category": Schema.optionalKey(Schema.String),
  "title": Schema.optionalKey(Schema.String),
  "titleLineId": Schema.optionalKey(Schema.String),
  "series": Schema.optionalKey(Schema.String),
  "order": Schema.optionalKey(Schema.Int
  .pipe(
    Schema.check(Schema.isGreaterThanOrEqualTo(0))
  )),
  "when": Schema.optionalKey(Schema.suspend(() => celSlot)),
  "body": Schema.suspend(() => position),
  "on": Schema.optionalKey(Schema.String
  .pipe(
    Schema.check(Schema.isPattern(new RegExp("^[A-Za-z0-9_][A-Za-z0-9_-]*$")))
  )),
  "priority": Schema.optionalKey(Schema.Int),
  "once": Schema.optionalKey(Schema.Union([
    Schema.Literals([
      "run",
      "user",
      "day",
      "slot",
      "week"
    ]),
    Schema.String
    .pipe(
      Schema.check(Schema.isPattern(new RegExp("^season:[A-Za-z0-9_][A-Za-z0-9_-]*$")))
    )
  ])),
  "share": Schema.optionalKey(Schema.String
  .pipe(
    Schema.check(Schema.isPattern(new RegExp("^[A-Za-z0-9_][A-Za-z0-9_-]*$")))
  )),
  "targetKind": Schema.optionalKey(Schema.suspend(() => targetKind)),
  "forKind": Schema.optionalKey(Schema.suspend(() => forKind)),
  "spentBy": Schema.optionalKey(Schema.suspend(() => celSlot)),
  "advances": Schema.optionalKey(Schema.Union([
    Schema.Literals([
      "slot",
      "day"
    ]),
    Schema.Int
    .pipe(
      Schema.check(Schema.isGreaterThanOrEqualTo(1))
    )
  ])),
  "timing": Schema.optionalKey(Schema.suspend(() => timing)),
  "provenance": Schema.optionalKey(Schema.suspend(() => provenance)),
  "source": Schema.optionalKey(Schema.suspend(() => source))
});
export type cmdEntry = typeof cmdEntry.Type;

/**
 * `::accept{quest}` (dsl 0.21.0 §7a.3): the player accepts an accept-driven quest at this point — the scene-side form of the engine's accept action. The engine activates the quest if its state is `unset` (stamping `activatedAt` as for any activation) and ignores the record otherwise (docs/runtime/quest-lifecycle.md).
 */
export const cmdAccept = Schema.Struct({
  "kind": Schema.Literal("accept"),
  "family": Schema.Literal("declaration"),
  "position": Schema.suspend(() => position),
  "quest": Schema.String,
  "applies": Schema.optionalKey(Schema.Literals([
    "nextRun"
  ])),
  "timing": Schema.optionalKey(Schema.suspend(() => timing)),
  "provenance": Schema.optionalKey(Schema.suspend(() => provenance)),
  "source": Schema.optionalKey(Schema.suspend(() => source))
});
export type cmdAccept = typeof cmdAccept.Type;

/**
 * A bundle beat's declaration head in a lore artifact (dsl 0.23.0 §4, docs/runtime/beats-and-occasions.md): a scene-like beat written in a lore document. It is the first record of its own addressing unit; the engine presents its body like a scene when the beat wins its occasion.
 */
export const cmdBeat = Schema.Struct({
  "kind": Schema.Literal("beat"),
  "family": Schema.Literal("declaration"),
  "position": Schema.suspend(() => position),
  "id": Schema.String,
  "on": Schema.String
  .pipe(
    Schema.check(Schema.isPattern(new RegExp("^[A-Za-z0-9_][A-Za-z0-9_-]*$")))
  ),
  "target": Schema.optionalKey(Schema.String),
  "title": Schema.optionalKey(Schema.String),
  "titleLineId": Schema.optionalKey(Schema.String),
  "when": Schema.optionalKey(Schema.suspend(() => celSlot)),
  "priority": Schema.Int,
  "once": Schema.Union([
    Schema.Literals([
      "run",
      "user",
      "none",
      "day",
      "slot",
      "week"
    ]),
    Schema.String
    .pipe(
      Schema.check(Schema.isPattern(new RegExp("^season:[A-Za-z0-9_][A-Za-z0-9_-]*$")))
    )
  ]),
  "also": Schema.optionalKey(Schema.Literal(true)),
  "share": Schema.optionalKey(Schema.String
  .pipe(
    Schema.check(Schema.isPattern(new RegExp("^[A-Za-z0-9_][A-Za-z0-9_-]*$")))
  )),
  "after": Schema.optionalKey(Schema.String),
  "targetKind": Schema.optionalKey(Schema.suspend(() => targetKind)),
  "forKind": Schema.optionalKey(Schema.suspend(() => forKind)),
  "spentBy": Schema.optionalKey(Schema.suspend(() => celSlot)),
  "advances": Schema.optionalKey(Schema.Union([
    Schema.Literals([
      "slot",
      "day"
    ]),
    Schema.Int
    .pipe(
      Schema.check(Schema.isGreaterThanOrEqualTo(1))
    )
  ])),
  "body": Schema.suspend(() => position),
  "timing": Schema.optionalKey(Schema.suspend(() => timing)),
  "provenance": Schema.optionalKey(Schema.suspend(() => provenance)),
  "source": Schema.optionalKey(Schema.suspend(() => source))
});
export type cmdBeat = typeof cmdBeat.Type;

/**
 * Plugin-directive passthrough (Rust `Command::Plugin`), serialized as kind `plugin`. Declared fields and effects are opaque to core. Plugin `stampAttrs` cross-cutting attrs appear as further top-level keys beside `timing`.
 */
export const cmdPlugin = Schema.Struct({
  "kind": Schema.Literal("plugin"),
  "family": Schema.Literal("plugin"),
  "position": Schema.suspend(() => position),
  "tag": Schema.String,
  "plugin": Schema.optionalKey(Schema.String),
  "fields": Schema.Json,
  "effects": Schema.optionalKey(Schema.Array(Schema.suspend(() => effect))),
  "retracts": Schema.optionalKey(Schema.Array(Schema.Struct({
    "relation": Schema.String,
    "args": Schema.Array(Schema.String)
  }))),
  "asserts": Schema.optionalKey(Schema.Array(Schema.Struct({
    "relation": Schema.String,
    "args": Schema.Array(Schema.String)
  }))),
  "timing": Schema.optionalKey(Schema.suspend(() => timing)),
  "provenance": Schema.optionalKey(Schema.suspend(() => provenance)),
  "source": Schema.optionalKey(Schema.suspend(() => source))
});
export type cmdPlugin = typeof cmdPlugin.Type;

/**
 * Kind-polymorphic envelope meta (untagged): SceneMeta when kind==scene, QuestMeta when kind==quest, LoreMeta when kind==lore. The shapes overlap (a bare `{"id": …}` fits all three, since dsl 0.19.0 §2.1 gives quest and lore documents an optional `id`), so this is `anyOf`, not `oneOf`: the top-level allOf pins the exact one by `kind`.
 */
export const artifactMeta = Schema.Union([
  Schema.suspend(() => sceneMeta),
  Schema.suspend(() => questMeta),
  Schema.suspend(() => loreMeta)
]);
export type artifactMeta = typeof artifactMeta.Type;

/**
 * Lore-kind envelope meta (dsl 0.19.0 §7): `id?`, `title?`, `series?`, `contentLang?`, `extra?`, `plugin?` — every field skipped when unauthored, so it MAY serialize as {}. Present when `kind` == "lore".
 */
export const loreMeta = Schema.Struct({
  "id": Schema.optionalKey(Schema.String
  .pipe(
    Schema.check(Schema.isPattern(new RegExp("^[A-Za-z0-9_][A-Za-z0-9_-]*(\\.[A-Za-z0-9_][A-Za-z0-9_-]*)*$")))
  )),
  "title": Schema.optionalKey(Schema.String),
  "series": Schema.optionalKey(Schema.String
  .pipe(
    Schema.check(Schema.isPattern(new RegExp("^[A-Za-z0-9_][A-Za-z0-9_-]*$")))
  )),
  "contentLang": Schema.optionalKey(Schema.String),
  "extra": Schema.optionalKey(Schema.Json),
  "plugin": Schema.optionalKey(Schema.Json)
});
export type loreMeta = typeof loreMeta.Type;

/**
 * One `<entry>` row of `ProjectIndex::entries` (dsl 0.19.0 §7): enough for
 * an engine to build its `target → entries` / `series → entries` tables
 * without loading every lore artifact. `document` is the SAME string as the
 * owning `IndexDocument::path`.
 */
export const indexEntry = Schema.Struct({
  "category": Schema.optionalKey(Schema.String),
  "document": Schema.String,
  "id": Schema.String,
  "order": Schema.optionalKey(Schema.Int
  .pipe(
    Schema.check(Schema.isGreaterThanOrEqualTo(0))
  )),
  "series": Schema.optionalKey(Schema.String),
  "target": Schema.optionalKey(Schema.String)
});
export type indexEntry = typeof indexEntry.Type;

/**
 * One row of `ProjectIndex::beats` (dsl 0.21.0 §4/§8): every beat in the
 * project, so an engine can build its `occasion → candidates` table without
 * loading every artifact. Row order IS the selection tiebreak after
 * priority. `id` is the scene's canonical id (`SceneMeta::id`), the
 * entry id, or a bundle beat's canonical `<document id>.<beat id>`;
 * `document` is the owning `IndexDocument::path`; `priority` is
 * resolved (unauthored → `0`); `once` is the scene's / bundle beat's policy,
 * or an entry's authored `once` (dsl 0.22.0 §7) — absent on an entry row =
 * repeatable.
 * `when` is the beat's condition after `@def` expansion (a scene's
 * `meta.beat.when`, an entry's own `when`) and `title` the scene's / entry's
 * title — enough to list the beats and label a `select: all` menu without
 * loading every artifact (dsl 0.23.0 §1, §11). Both are omitted when absent.
 */
export const indexBeat = Schema.Struct({
  "advances": Schema.optionalKey(Schema.Union([
    Schema.Literals([
      "slot",
      "day"
    ]),
    Schema.Int
    .pipe(
      Schema.check(Schema.isGreaterThanOrEqualTo(1))
    )
  ])),
  "document": Schema.String,
  "forKind": Schema.optionalKey(Schema.suspend(() => forKind)),
  "id": Schema.String,
  "kind": Schema.Literals([
    "scene",
    "entry",
    "bundle"
  ]),
  "on": Schema.String,
  "once": Schema.optionalKey(Schema.String
  .pipe(
    Schema.check(Schema.isPattern(new RegExp("^(run|user|none|day|slot|week|season:.+)$")))
  )),
  "priority": Schema.Int,
  "share": Schema.optionalKey(Schema.String),
  "spentBy": Schema.optionalKey(Schema.String),
  "target": Schema.optionalKey(Schema.String),
  "targetKind": Schema.optionalKey(Schema.suspend(() => targetKind)),
  "title": Schema.optionalKey(Schema.String),
  "when": Schema.optionalKey(Schema.String)
});
export type indexBeat = typeof indexBeat.Type;

/**
 * dsl 0.37.0 §5.3: one authored document section. Purely descriptive — control flow never references it.
 */
export const sectionEntry = Schema.Struct({
  "section": Schema.Int
  .pipe(
    Schema.check(Schema.isGreaterThanOrEqualTo(1))
  ),
  "heading": Schema.String,
  "id": Schema.optionalKey(Schema.String
  .pipe(
    Schema.check(Schema.isPattern(new RegExp("^[A-Za-z][A-Za-z0-9_-]{0,63}$")))
  ))
});
export type sectionEntry = typeof sectionEntry.Type;

/**
 * dsl 0.24.0 §1: the project's declared clock, verbatim from the schema's `clock:` block. Carried on the artifact root and on `project.index.json` (`lute compile --all`) as `clock`. The engine derives the reserved read-only paths from it — `clock.index` = `(day - 1) * len(slots) + slotIndex` (`day - 1` without slots; monotone), `clock.weekday` = `(week.first + day - 1) mod week.length`, `clock.weekdayLabel` = `week.labelsclock.weekday` — none of which is a `state` row, and spends `once: day|slot` beats by it. An advance moves the clock forward only (the last slot wraps to the first slot of the next day), settles quests and raises `raise` (`dayEnd` / `dayStart` at each midnight it crosses, `slot` where it stops) when declared. dsl 0.27.0 §4: a finite clock (`last` / `days`) stops at its last position — an advance whose destination lies past it walks there (raising `dayEnd` / `dayStart` at each midnight it crosses on the way), raises the last day's `dayEnd` (never `slot`) and ends the clock; any later advance is the usage error `E-CLOCK-END` until a new run starts the clock over.
 */
export const clockDecl = Schema.Struct({
  "day": Schema.String,
  "slot": Schema.optionalKey(Schema.String),
  "slots": Schema.optionalKey(Schema.Array(Schema.String)),
  "raise": Schema.optionalKey(Schema.Union([
    Schema.String
    .pipe(
      Schema.check(Schema.isPattern(new RegExp("^[A-Za-z0-9_][A-Za-z0-9_-]*$")))
    ),
    Schema.Struct({
      "slot": Schema.optionalKey(Schema.String
      .pipe(
        Schema.check(Schema.isPattern(new RegExp("^[A-Za-z0-9_][A-Za-z0-9_-]*$")))
      )),
      "dayStart": Schema.optionalKey(Schema.String
      .pipe(
        Schema.check(Schema.isPattern(new RegExp("^[A-Za-z0-9_][A-Za-z0-9_-]*$")))
      )),
      "dayEnd": Schema.optionalKey(Schema.String
      .pipe(
        Schema.check(Schema.isPattern(new RegExp("^[A-Za-z0-9_][A-Za-z0-9_-]*$")))
      ))
    })
  ])),
  "raiseAtStart": Schema.optionalKey(Schema.Literal(true)),
  "week": Schema.optionalKey(Schema.Struct({
    "length": Schema.Int
    .pipe(
      Schema.check(Schema.isGreaterThanOrEqualTo(1))
    ),
    "first": Schema.Int
    .pipe(
      Schema.check(Schema.isGreaterThanOrEqualTo(0))
    ),
    "labels": Schema.optionalKey(Schema.Array(Schema.String))
  })),
  "last": Schema.optionalKey(Schema.Struct({
    "day": Schema.Int
    .pipe(
      Schema.check(Schema.isGreaterThanOrEqualTo(1))
    ),
    "slot": Schema.optionalKey(Schema.String)
  })),
  "days": Schema.optionalKey(Schema.Int
  .pipe(
    Schema.check(Schema.isGreaterThanOrEqualTo(1))
  ))
});
export type clockDecl = typeof clockDecl.Type;

/**
 * The serializable subset of a capability-declared occasion carried by the
 * project index. The human description and raisedWhen gate are document
 * metadata, not part of the engine's occasion shape.
 */
export const indexOccasion = Schema.Struct({
  "judge": Schema.optionalKey(Schema.Literals([
    "after",
    "before"
  ])),
  "outsideRun": Schema.optionalKey(Schema.Boolean),
  "payload": Schema.optionalKey(Schema.Record(Schema.String, Schema.suspend(() => fieldType))),
  "select": Schema.Literals([
    "first",
    "all",
    "sequence"
  ]),
  "target": Schema.Union([
    Schema.Boolean,
    Schema.Struct({
      "entity": Schema.String,
      "members": Schema.optionalKey(Schema.Array(Schema.String)),
      "prefix": Schema.String
    })
  ])
});
export type indexOccasion = typeof indexOccasion.Type;

/**
 * A closed state member domain, keyed by state path in
 * `ProjectIndex::state_domains`.
 */
export const indexStateDomain = Schema.Struct({
  "kind": Schema.String,
  "members": Schema.Array(Schema.String)
});
export type indexStateDomain = typeof indexStateDomain.Type;

/**
 * `project.index.json` (`lute compile --all`): the document table and the union of every compiled document's vocabulary an engine needs before it evaluates anything — the six vocabulary arrays always present, every other key omitted when empty (docs/runtime/execution-model.md).
 */
export const projectIndex = Schema.Struct({
  "beats": Schema.optionalKey(Schema.Array(Schema.suspend(() => indexBeat))),
  "bridgeResults": Schema.optionalKey(Schema.Record(Schema.String, Schema.Record(Schema.String, Schema.suspend(() => fieldType)))),
  "capabilitySnapshot": Schema.String,
  "cast": Schema.optionalKey(Schema.Record(Schema.String, Schema.String)),
  "clock": Schema.optionalKey(Schema.suspend(() => clockDecl)),
  "defs": Schema.optionalKey(Schema.Record(Schema.String, Schema.suspend(() => indexDef))),
  "documents": Schema.Array(Schema.suspend(() => indexDocument)),
  "entities": Schema.Array(Schema.suspend(() => entityKindEntry)),
  "entries": Schema.optionalKey(Schema.Array(Schema.suspend(() => indexEntry))),
  "enums": Schema.Array(Schema.suspend(() => enumEntry)),
  "gates": Schema.optionalKey(Schema.Array(Schema.Struct({
    "occasion": Schema.String,
    "raisedWhen": Schema.suspend(() => celSlot)
  }))),
  "identityRenames": Schema.optionalKey(Schema.Array(Schema.Struct({
    "from": Schema.String,
    "to": Schema.String
  }))),
  "irVersion": Schema.String,
  "occasions": Schema.optionalKey(Schema.Record(Schema.String, Schema.suspend(() => indexOccasion))),
  "outsideRun": Schema.optionalKey(Schema.Array(Schema.String)),
  "prereqEdges": Schema.Array(Schema.suspend(() => prereqEdgeEntry)),
  "relations": Schema.Array(Schema.suspend(() => relationEntry)),
  "requiredSemantics": Schema.Array(Schema.String),
  "rules": Schema.Array(Schema.suspend(() => ruleEntry)),
  "seasons": Schema.optionalKey(Schema.Array(Schema.Struct({
    "live": Schema.suspend(() => celSlot),
    "name": Schema.String
  }))),
  "seedFacts": Schema.Array(Schema.suspend(() => seedFactEntry)),
  "stateDomains": Schema.optionalKey(Schema.Record(Schema.String, Schema.suspend(() => indexStateDomain))),
  "terminal": Schema.optionalKey(Schema.suspend(() => celSlot)),
  "terminalPersists": Schema.optionalKey(Schema.Boolean),
  "worldEvents": Schema.optionalKey(Schema.Array(Schema.String))
});
export type projectIndex = typeof projectIndex.Type;

/**
 * One document's row in the index. Paths are FORWARD-SLASH relative to the
 * project root, never absolute — an index is a build output that must survive
 * being copied to another machine or shipped inside a game package.
 */
export const indexDocument = Schema.Struct({
  "artifact": Schema.String,
  "key": Schema.String,
  "kind": Schema.Literals([
    "scene",
    "quest",
    "lore"
  ]),
  "path": Schema.String
});
export type indexDocument = typeof indexDocument.Type;

/**
 * 0.41.0 §4.3: one row of `ProjectIndex::defs` — a def's body
 * `@def`-expanded (`cel`; the unexpanded body as `authored` when it
 * differs) and lowered (`expr`, where each parameter is a `param` node),
 * its parameters in declaration order (omitted when none) and `host: true`
 * when the def declares it (omitted otherwise).
 */
export const indexDef = Schema.Struct({
  "authored": Schema.optionalKey(Schema.String),
  "cel": Schema.String,
  "expr": Schema.Json,
  "host": Schema.optionalKey(Schema.Boolean),
  "params": Schema.optionalKey(Schema.Array(Schema.Struct({
    "name": Schema.String,
    "type": Schema.suspend(() => fieldType)
  })))
});
export type indexDef = typeof indexDef.Type;

/**
 * A declared value type (plugin §7) as the IR serializes it: a bare spelling (`bool`, `int`, `double`, `string`, `narrativeTime`) or a single-key map naming the form (`{"enum": ...}`, `{"list": T}`, `{"record": fields}`, `{"map": {"key": T, "value": T}}`, `{"domain": name}`, `{"entity": kind}`, `{"enumFromOption": option}`, `{"providerRef": name}`, `{"slotId": {"namespace": ns}}`, `{"assetKind": kind}`).
 */
export type fieldType =
  | ("bool" | "int" | "double" | "string" | "narrativeTime")
  | {
    readonly "enum": ReadonlyArray<string>
  }
  | {
    readonly "list": fieldType
  }
  | {
    readonly "record": ReadonlyArray<{
      readonly "default"?: Schema.Json,
      readonly "name": string,
      readonly "required": boolean,
      readonly "shape"?: string,
      readonly "type": fieldType
    }>
  }
  | {
    readonly "map": {
      readonly "key": fieldType,
      readonly "value": fieldType
    }
  }
  | {
    readonly "enumFromOption": string
  }
  | {
    readonly "providerRef": string
  }
  | {
    readonly "domain": string
  }
  | {
    readonly "entity": string
  }
  | {
    readonly "slotId": {
      readonly "namespace": string
    }
  }
  | {
    readonly "assetKind": string
  };
export const fieldType: Schema.Codec<fieldType> = Schema.Union([
  Schema.Literals([
    "bool",
    "int",
    "double",
    "string",
    "narrativeTime"
  ]),
  Schema.Struct({
    "enum": Schema.Array(Schema.String)
  }),
  Schema.Struct({
    "list": Schema.suspend(() => fieldType)
  }),
  Schema.Struct({
    "record": Schema.Array(Schema.Struct({
      "default": Schema.optionalKey(Schema.Json),
      "name": Schema.String,
      "required": Schema.Boolean,
      "shape": Schema.optionalKey(Schema.String),
      "type": Schema.suspend(() => fieldType)
    }))
  }),
  Schema.Struct({
    "map": Schema.Struct({
      "key": Schema.suspend(() => fieldType),
      "value": Schema.suspend(() => fieldType)
    })
  }),
  Schema.Struct({
    "enumFromOption": Schema.String
  }),
  Schema.Struct({
    "providerRef": Schema.String
  }),
  Schema.Struct({
    "domain": Schema.String
  }),
  Schema.Struct({
    "entity": Schema.String
  }),
  Schema.Struct({
    "slotId": Schema.Struct({
      "namespace": Schema.String
    })
  }),
  Schema.Struct({
    "assetKind": Schema.String
  })
]);

/**
 * One command record (dsl 0.37.0 §5.3), tagged on `kind` with its `family`. An engine MUST error on an unknown `kind` (version-negotiation policy, top-level description); it MUST ignore unknown FIELDS within a known record.
 */
export const Command = Schema.Union([
  Schema.suspend(() => cmdLine),
  Schema.suspend(() => cmdBg),
  Schema.suspend(() => cmdMusic),
  Schema.suspend(() => cmdSfx),
  Schema.suspend(() => cmdVfx),
  Schema.suspend(() => cmdActor),
  Schema.suspend(() => cmdCamera),
  Schema.suspend(() => cmdCg),
  Schema.suspend(() => cmdVideo),
  Schema.suspend(() => cmdSequence),
  Schema.suspend(() => cmdSet),
  Schema.suspend(() => cmdAssert),
  Schema.suspend(() => cmdRetract),
  Schema.suspend(() => cmdChoice),
  Schema.suspend(() => cmdMatch),
  Schema.suspend(() => cmdHub),
  Schema.suspend(() => cmdJump),
  Schema.suspend(() => cmdEnd),
  Schema.suspend(() => cmdBarrier),
  Schema.suspend(() => cmdQuest),
  Schema.suspend(() => cmdOn),
  Schema.suspend(() => cmdEntry),
  Schema.suspend(() => cmdAccept),
  Schema.suspend(() => cmdBeat),
  Schema.suspend(() => cmdPlugin)
]);
export type Command = typeof Command.Type;

