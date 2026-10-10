/**
 * A compiled bundle: `project.index.json` and every artifact it lists, keyed
 * by the index's `artifact` paths — the output of `lute compile --all`
 * (spec 0.38.0 §4.1). The runtime validates it on load.
 */
import { Schema } from "effect"

export const Bundle = Schema.Struct({
  index: Schema.Json,
  artifacts: Schema.Record(Schema.String, Schema.Json)
})
export type Bundle = typeof Bundle.Type

/** The part of `project.index.json` a loader reads: where each artifact is. */
export const IndexDocuments = Schema.Struct({
  documents: Schema.Array(Schema.Struct({ artifact: Schema.String }))
})
