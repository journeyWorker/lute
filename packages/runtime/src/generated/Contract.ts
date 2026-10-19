// THIS FILE IS GENERATED — DO NOT EDIT.
// Generator: packages/runtime/scripts/contract-codegen.ts
// Sources: schemas/lute-events-0.48.schema.json, schemas/lute-snapshot-0.48.schema.json

import * as Schema from "effect/Schema";
import type * as SchemaAST from "effect/SchemaAST";

export const parseOptions = { onExcessProperty: "error" } as const satisfies SchemaAST.ParseOptions;

export const AdvanceBy = Schema.Union([
  Schema.String,
  Schema.Int
  .pipe(
    Schema.check(Schema.isGreaterThanOrEqualTo(0))
  ),
  Schema.Struct({
    "to": Schema.String
  }),
  Schema.Struct({
    "to": Schema.suspend(() => ClockPosition)
  })
]);
export type AdvanceBy = typeof AdvanceBy.Type;

/**
 * A single leaf condition flattened out of a `PrereqFormula` by `atoms`
 * (edge-extraction helper for later connectivity tasks).
 */
export const Atom = Schema.Union([
  Schema.Struct({
    "visited": Schema.String
  }),
  Schema.Struct({
    "completed": Schema.String
  }),
  Schema.Struct({
    "active": Schema.String
  })
]);
export type Atom = typeof Atom.Type;

export const Await = Schema.Union([
  Schema.Struct({
    "menu": Schema.suspend(() => MenuAwait),
    "request": Schema.Int
    .pipe(
      Schema.check(Schema.isGreaterThanOrEqualTo(0))
    ),
    "type": Schema.Literal("awaitChoice")
  }),
  Schema.Struct({
    "attrs": Schema.Record(Schema.String, Schema.Json),
    "document": Schema.String,
    "position": Schema.String,
    "request": Schema.Int
    .pipe(
      Schema.check(Schema.isGreaterThanOrEqualTo(0))
    ),
    "results": Schema.Record(Schema.String, Schema.suspend(() => fieldType)),
    "tag": Schema.String,
    "type": Schema.Literal("awaitBridge")
  }),
  Schema.Struct({
    "type": Schema.Literal("idle")
  }),
  Schema.Struct({
    "reason": Schema.String,
    "type": Schema.Literal("ended")
  }),
  Schema.Struct({
    "kind": Schema.String,
    "message": Schema.String,
    "site": Schema.optionalKey(Schema.NullOr(Schema.String)),
    "type": Schema.Literal("halted")
  })
])
.pipe(
  Schema.toTaggedUnion("type")
);
export type Await = typeof Await.Type;

/**
 * What a `ProjectIndex::beats` row declares (dsl 0.21.0 §8): a scene beat
 * (`SceneMeta.beat`), an entry beat (`EntryCmd.on`), or a bundle beat (a
 * lore document's `beat` record, dsl 0.23.0 §4).
 */
export const BeatKind = Schema.Literals([
  "scene",
  "entry",
  "bundle"
]);
export type BeatKind = typeof BeatKind.Type;

export const BeatOnce = Schema.String
.pipe(
  Schema.check(Schema.isPattern(new RegExp("^(run|user|none|day|slot|week|season:.+)$")))
);
export type BeatOnce = typeof BeatOnce.Type;

export const Candidate = Schema.Struct({
  "also": Schema.Boolean,
  "document": Schema.String,
  "forMember": Schema.optionalKey(Schema.NullOr(Schema.String)),
  "id": Schema.String,
  "kind": Schema.suspend(() => BeatKind),
  "priority": Schema.Int,
  "read": Schema.Boolean,
  "rejudged": Schema.Boolean,
  "verdict": Schema.suspend(() => Verdict)
});
export type Candidate = typeof Candidate.Type;

/**
 * A position on the clock: a day and the index of a slot in `slots`.
 */
export const ClockAt = Schema.Struct({
  "day": Schema.Int,
  "slot": Schema.Int
  .pipe(
    Schema.check(Schema.isGreaterThanOrEqualTo(0))
  )
});
export type ClockAt = typeof ClockAt.Type;

export const ClockPosition = Schema.Struct({
  "slot": Schema.optionalKey(Schema.NullOr(Schema.String)),
  "weekday": Schema.optionalKey(Schema.NullOr(Schema.String))
});
export type ClockPosition = typeof ClockPosition.Type;

/**
 * Where the declared clock stands (dsl 0.26.0 §7, T2-5: step
 * `expect.clock`).
 */
export const ClockView = Schema.Struct({
  "day": Schema.Int,
  "ended": Schema.optionalKey(Schema.NullOr(Schema.Boolean)),
  "last": Schema.optionalKey(Schema.NullOr(Schema.String)),
  "slot": Schema.optionalKey(Schema.NullOr(Schema.String)),
  "weekday": Schema.optionalKey(Schema.NullOr(Schema.Int)),
  "weekdayLabel": Schema.optionalKey(Schema.NullOr(Schema.String))
});
export type ClockView = typeof ClockView.Type;

/**
 * The answer of a read-only condition query, `evaluate` or `def`
 * (dsl 0.41.0 §5.1).
 */
export const Evaluation = Schema.Struct({
  "unresolved": Schema.optionalKey(Schema.Array(Schema.String)),
  "value": Schema.suspend(() => Value)
});
export type Evaluation = typeof Evaluation.Type;

/**
 * An execution record or session-level notification.
 */
export const Event = Schema.Union([
  Schema.Struct({
    "document": Schema.String,
    "record": Schema.Json,
    "type": Schema.Literal("record")
  }),
  Schema.Struct({
    "beat": Schema.String,
    "document": Schema.String,
    "kind": Schema.String,
    "occasion": Schema.optionalKey(Schema.NullOr(Schema.String)),
    "target": Schema.optionalKey(Schema.NullOr(Schema.String)),
    "type": Schema.Literal("presentation")
  }),
  Schema.Struct({
    "beat": Schema.String,
    "document": Schema.String,
    "reason": Schema.optionalKey(Schema.NullOr(Schema.String)),
    "type": Schema.Literal("presentationEnd")
  }),
  Schema.Struct({
    "commands": Schema.Array(Schema.Json),
    "document": Schema.String,
    "type": Schema.Literal("quest")
  }),
  Schema.Struct({
    "from": Schema.Json,
    "passed": Schema.optionalKey(Schema.Json),
    "to": Schema.Json,
    "type": Schema.Literal("clock")
  })
])
.pipe(
  Schema.toTaggedUnion("type")
);
export type Event = typeof Event.Type;

/**
 * dsl 0.41.0 §5.2: the answer of the `gate` query — whether a
 * `raiseOccasion` of the occasion (for its target) with no writes would
 * pass the seam now.
 */
export const Gate = Schema.Struct({
  "open": Schema.Boolean,
  "unresolved": Schema.optionalKey(Schema.Array(Schema.String))
});
export type Gate = typeof Gate.Type;

/**
 * One read of a guard that decided false, as the premise a refusal names
 * (round-5 T3-12): a state path (with the value it held), a fact pattern
 * that does not hold, a scene `visited(…)` has not seen — or, under a
 * negation (OT-F-10), a fact that holds, a scene that is visited.
 */
export const GuardRead = Schema.Union([
  Schema.Struct({
    "kind": Schema.Literal("path"),
    "value": Schema.Tuple([
      Schema.String,
      Schema.suspend(() => Value)
    ])
  }),
  Schema.Struct({
    "kind": Schema.Literal("fact"),
    "value": Schema.String
  }),
  Schema.Struct({
    "kind": Schema.Literal("derived"),
    "value": Schema.Struct({
      "base": Schema.Array(Schema.String),
      "fact": Schema.String,
      "rules": Schema.Array(Schema.Tuple([
        Schema.String,
        Schema.Array(Schema.String)
      ]))
    })
  }),
  Schema.Struct({
    "kind": Schema.Literal("visited"),
    "value": Schema.String
  }),
  Schema.Struct({
    "kind": Schema.Literal("holds"),
    "value": Schema.String
  }),
  Schema.Struct({
    "kind": Schema.Literal("seen"),
    "value": Schema.String
  })
])
.pipe(
  Schema.toTaggedUnion("kind")
);
export type GuardRead = typeof GuardRead.Type;

/**
 * The seven inputs in §5.1.
 */
export const Input = Schema.Union([
  Schema.Struct({
    "occasion": Schema.String,
    "payload": Schema.optionalKey(Schema.Record(Schema.String, Schema.Json)),
    "pick": Schema.optionalKey(Schema.NullOr(Schema.suspend(() => PickInput))),
    "target": Schema.optionalKey(Schema.NullOr(Schema.String)),
    "type": Schema.Literal("raiseOccasion"),
    "writes": Schema.optionalKey(Schema.suspend(() => WritesInput))
  }),
  Schema.Struct({
    "request": Schema.Int
    .pipe(
      Schema.check(Schema.isGreaterThanOrEqualTo(0))
    ),
    "type": Schema.Literal("timeout")
  }),
  Schema.Struct({
    "option": Schema.String,
    "request": Schema.Int
    .pipe(
      Schema.check(Schema.isGreaterThanOrEqualTo(0))
    ),
    "type": Schema.Literal("choose")
  }),
  Schema.Struct({
    "fields": Schema.Record(Schema.String, Schema.Json),
    "request": Schema.Int
    .pipe(
      Schema.check(Schema.isGreaterThanOrEqualTo(0))
    ),
    "type": Schema.Literal("bridgeResult"),
    "writes": Schema.optionalKey(Schema.suspend(() => WritesInput))
  }),
  Schema.Struct({
    "by": Schema.suspend(() => AdvanceBy),
    "pick": Schema.optionalKey(Schema.NullOr(Schema.suspend(() => PickInput))),
    "type": Schema.Literal("advanceClock"),
    "writes": Schema.optionalKey(Schema.suspend(() => WritesInput))
  }),
  Schema.Struct({
    "type": Schema.Literal("hostWrite"),
    "writes": Schema.suspend(() => WritesInput)
  }),
  Schema.Struct({
    "name": Schema.String,
    "type": Schema.Literal("worldEvent")
  }),
  Schema.Struct({
    "type": Schema.Literal("newRun"),
    "writes": Schema.optionalKey(Schema.suspend(() => WritesInput))
  })
])
.pipe(
  Schema.toTaggedUnion("type")
);
export type Input = typeof Input.Type;

export const MenuAwait = Schema.Struct({
  "construct": Schema.String,
  "default": Schema.optionalKey(Schema.NullOr(Schema.String)),
  "document": Schema.String,
  "id": Schema.String,
  "options": Schema.Array(Schema.suspend(() => MenuOptionAwait)),
  "position": Schema.String,
  "presentation": Schema.Int
  .pipe(
    Schema.check(Schema.isGreaterThanOrEqualTo(0))
  ),
  "prompt": Schema.optionalKey(Schema.NullOr(Schema.String)),
  "timeout": Schema.optionalKey(Schema.NullOr(Schema.Int
  .pipe(
    Schema.check(Schema.isGreaterThanOrEqualTo(0))
  )))
});
export type MenuAwait = typeof MenuAwait.Type;

export const MenuOptionAwait = Schema.Struct({
  "exit": Schema.Boolean,
  "id": Schema.String,
  "once": Schema.Boolean,
  "verdict": Schema.String
});
export type MenuOptionAwait = typeof MenuOptionAwait.Type;

export const Output = Schema.Struct({
  "await": Schema.suspend(() => Await),
  "eventVersion": Schema.String,
  "events": Schema.Array(Schema.suspend(() => Event))
});
export type Output = typeof Output.Type;

export const PickInput = Schema.Union([
  Schema.String,
  Schema.Struct({
    "beat": Schema.String
  })
]);
export type PickInput = typeof PickInput.Type;

export const Premise = Schema.Union([
  Schema.Struct({
    "once": Schema.optionalKey(Schema.NullOr(Schema.suspend(() => BeatOnce))),
    "premise": Schema.Literal("spent"),
    "reason": Schema.String
  }),
  Schema.Struct({
    "chapters": Schema.Boolean,
    "premise": Schema.Literal("after"),
    "raw": Schema.String,
    "unmet": Schema.Array(Schema.suspend(() => Atom))
  }),
  Schema.Struct({
    "premise": Schema.Literal("spentBy"),
    "raw": Schema.String
  }),
  Schema.Struct({
    "premise": Schema.Literal("when"),
    "raw": Schema.String
  }),
  Schema.Struct({
    "occasion": Schema.String,
    "premise": Schema.Literal("gate"),
    "raw": Schema.String,
    "reads": Schema.Array(Schema.suspend(() => GuardRead))
  }),
  Schema.Struct({
    "occasion": Schema.String,
    "premise": Schema.Literal("terminal"),
    "raw": Schema.String
  })
])
.pipe(
  Schema.toTaggedUnion("premise")
);
export type Premise = typeof Premise.Type;

export const Rejected = Schema.Struct({
  "code": Schema.String,
  "message": Schema.String
});
export type Rejected = typeof Rejected.Type;

export const SaveInput = Schema.Struct({
  "entriesRead": Schema.optionalKey(Schema.Record(Schema.String, Schema.Array(Schema.String))),
  "presented": Schema.optionalKey(Schema.Record(Schema.String, Schema.Array(Schema.String))),
  "questInstances": Schema.optionalKey(Schema.Record(Schema.String, Schema.Int
  .pipe(
    Schema.check(Schema.isGreaterThanOrEqualTo(0))
  ))),
  "quests": Schema.optionalKey(Schema.Record(Schema.String, Schema.String)),
  "visited": Schema.optionalKey(Schema.Array(Schema.String))
});
export type SaveInput = typeof SaveInput.Type;

/**
 * The initial world seed (§5.1).
 */
export const Seed = Schema.Struct({
  "derive": Schema.optionalKey(Schema.Boolean),
  "facts": Schema.optionalKey(Schema.Array(Schema.String)),
  "save": Schema.optionalKey(Schema.suspend(() => SaveInput)),
  "state": Schema.optionalKey(Schema.Array(Schema.suspend(() => StateWrite)))
});
export type Seed = typeof Seed.Type;

export const StateWrite = Schema.Struct({
  "add": Schema.optionalKey(Schema.NullOr(Schema.Number)),
  "path": Schema.String,
  "value": Schema.optionalKey(Schema.Json)
});
export type StateWrite = typeof StateWrite.Type;

/**
 * One line of the `--events` JSONL stream.
 */
export const StreamLine = Schema.Union([
  Schema.Struct({
    "output": Schema.suspend(() => Output),
    "seed": Schema.suspend(() => Seed)
  }),
  Schema.Struct({
    "input": Schema.suspend(() => Input),
    "output": Schema.suspend(() => Output)
  }),
  Schema.Struct({
    "input": Schema.suspend(() => Input),
    "rejected": Schema.suspend(() => Rejected)
  }),
  Schema.Struct({
    "rejected": Schema.suspend(() => Rejected),
    "seed": Schema.suspend(() => Seed)
  })
]);
export type StreamLine = typeof StreamLine.Type;

/**
 * Three-valued trace value (§4.3). The explicit tag preserves CEL integer
 * versus double values in JSON.
 */
export const Value = Schema.Union([
  Schema.Struct({
    "kind": Schema.Literal("bool"),
    "value": Schema.Boolean
  }),
  Schema.Struct({
    "kind": Schema.Literal("int"),
    "value": Schema.Int
  }),
  Schema.Struct({
    "kind": Schema.Literal("double"),
    "value": Schema.Number
  }),
  Schema.Struct({
    "kind": Schema.Literal("str"),
    "value": Schema.String
  }),
  Schema.Struct({
    "kind": Schema.Literal("unknown")
  }),
  Schema.Struct({
    "kind": Schema.Literal("error"),
    "value": Schema.String
  })
])
.pipe(
  Schema.toTaggedUnion("kind")
);
export type Value = typeof Value.Type;

export const Verdict = Schema.Union([
  Schema.Struct({
    "verdict": Schema.Literal("eligible")
  }),
  Schema.Struct({
    "premise": Schema.suspend(() => Premise),
    "verdict": Schema.Literal("ineligible")
  }),
  Schema.Struct({
    "verdict": Schema.Literal("unknown"),
    "why": Schema.String
  })
])
.pipe(
  Schema.toTaggedUnion("verdict")
);
export type Verdict = typeof Verdict.Type;

/**
 * The world at one moment of a play: the effective state, every fact that
 * holds after derivation (rendered `rel(a, b)`), every declared quest's
 * status, and the declared clock's position.
 */
export const WorldView = Schema.Struct({
  "clock": Schema.optionalKey(Schema.NullOr(Schema.suspend(() => ClockView))),
  "facts": Schema.optionalKey(Schema.UniqueArray(Schema.String)),
  "quests": Schema.Record(Schema.String, Schema.String),
  "state": Schema.Record(Schema.String, Schema.suspend(() => Value))
});
export type WorldView = typeof WorldView.Type;

export const WritesInput = Schema.Struct({
  "accept": Schema.optionalKey(Schema.Array(Schema.String)),
  "facts": Schema.optionalKey(Schema.Array(Schema.String)),
  "retract": Schema.optionalKey(Schema.Array(Schema.String)),
  "state": Schema.optionalKey(Schema.Array(Schema.suspend(() => StateWrite)))
});
export type WritesInput = typeof WritesInput.Type;

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

export const Answer = Schema.Union([
  Schema.Struct({
    "choice": Schema.Struct({
      "id": Schema.String,
      "option": Schema.String
    })
  }),
  Schema.Struct({
    "bridge": Schema.Struct({
      "fields": Schema.Record(Schema.String, Schema.Json),
      "tag": Schema.String,
      "writes": Schema.optionalKey(Schema.suspend(() => WritesInput))
    })
  })
]);
export type Answer = typeof Answer.Type;

/**
 * The cadence memory a playthrough carries between steps.
 */
export const Cadence = Schema.Struct({
  "latched": Schema.Record(Schema.String, Schema.NullOr(Schema.suspend(() => ClockAt))),
  "live": Schema.Record(Schema.String, Schema.Boolean),
  "rearm": Schema.Record(Schema.String, Schema.Boolean),
  "spentSeason": Schema.UniqueArray(Schema.String),
  "windows": Schema.UniqueArray(Schema.String)
});
export type Cadence = typeof Cadence.Type;

export const Continuation = Schema.Struct({
  "answers": Schema.Array(Schema.suspend(() => Answer)),
  "before": Schema.suspend(() => World),
  "delivered": Schema.Int
  .pipe(
    Schema.check(Schema.isGreaterThanOrEqualTo(0))
  ),
  "input": Schema.suspend(() => Input),
  "pending": Schema.suspend(() => Await),
  "stage": Schema.suspend(() => Stage)
});
export type Continuation = typeof Continuation.Type;

export const Stage = Schema.Struct({
  "actors": Schema.Array(Schema.suspend(() => StageActor)),
  "bg": Schema.optionalKey(Schema.NullOr(Schema.suspend(() => StageBackground))),
  "cg": Schema.optionalKey(Schema.NullOr(Schema.suspend(() => StageCg))),
  "music": Schema.optionalKey(Schema.NullOr(Schema.suspend(() => StageMusic))),
  "video": Schema.optionalKey(Schema.NullOr(Schema.suspend(() => StageVideo)))
});
export type Stage = typeof Stage.Type;

export const StageActor = Schema.Struct({
  "anchor": Schema.optionalKey(Schema.NullOr(Schema.String)),
  "character": Schema.String,
  "costume": Schema.optionalKey(Schema.NullOr(Schema.String)),
  "emotion": Schema.optionalKey(Schema.NullOr(Schema.String))
});
export type StageActor = typeof StageActor.Type;

export const StageBackground = Schema.Struct({
  "assetId": Schema.optionalKey(Schema.NullOr(Schema.String)),
  "location": Schema.optionalKey(Schema.NullOr(Schema.String)),
  "time": Schema.optionalKey(Schema.NullOr(Schema.String))
});
export type StageBackground = typeof StageBackground.Type;

export const StageCg = Schema.Struct({
  "assetId": Schema.String,
  "layout": Schema.optionalKey(Schema.NullOr(Schema.String))
});
export type StageCg = typeof StageCg.Type;

export const StageMusic = Schema.Struct({
  "assetId": Schema.optionalKey(Schema.NullOr(Schema.String)),
  "mood": Schema.optionalKey(Schema.NullOr(Schema.String)),
  "playback": Schema.optionalKey(Schema.NullOr(Schema.String)),
  "volume": Schema.optionalKey(Schema.NullOr(Schema.String))
});
export type StageMusic = typeof StageMusic.Type;

export const StageVideo = Schema.Struct({
  "assetId": Schema.String
});
export type StageVideo = typeof StageVideo.Type;

export const World = Schema.Struct({
  "accepts": Schema.Array(Schema.String),
  "advanceCascadeDepth": Schema.Int
  .pipe(
    Schema.check(Schema.isGreaterThanOrEqualTo(0))
  ),
  "cadence": Schema.suspend(() => Cadence),
  "clockAdvancedByBeat": Schema.Boolean,
  "deferBy": Schema.optionalKey(Schema.NullOr(Schema.String)),
  "deferHandlers": Schema.Boolean,
  "deferredHandlers": Schema.Array(Schema.Tuple([
    Schema.String,
    Schema.String
  ])),
  "derive": Schema.optionalKey(Schema.NullOr(Schema.Boolean)),
  "facts": Schema.UniqueArray(Schema.Tuple([
    Schema.String,
    Schema.Array(Schema.String)
  ])),
  "failedObjectives": Schema.UniqueArray(Schema.String),
  "nextRunAccepts": Schema.Array(Schema.String),
  "questInstances": Schema.Record(Schema.String, Schema.Int
  .pipe(
    Schema.check(Schema.isGreaterThanOrEqualTo(0))
  )),
  "quests": Schema.Record(Schema.String, Schema.String),
  "shareSpentBy": Schema.Record(Schema.String, Schema.String),
  "spentAt": Schema.Record(Schema.String, Schema.suspend(() => ClockAt)),
  "spentRun": Schema.UniqueArray(Schema.String),
  "spentUser": Schema.UniqueArray(Schema.String),
  "stage": Schema.suspend(() => Stage),
  "state": Schema.Record(Schema.String, Schema.suspend(() => Value)),
  "visited": Schema.UniqueArray(Schema.String)
});
export type World = typeof World.Type;

/**
 * A serialized, resumable runtime state.
 */
export const Snapshot = Schema.Struct({
  "await": Schema.suspend(() => Await),
  "continuation": Schema.optionalKey(Schema.NullOr(Schema.suspend(() => Continuation))),
  "project": Schema.String,
  "request": Schema.Int
  .pipe(
    Schema.check(Schema.isGreaterThanOrEqualTo(0))
  ),
  "snapshotVersion": Schema.String,
  "world": Schema.suspend(() => World)
});
export type Snapshot = typeof Snapshot.Type;

