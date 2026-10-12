/**
 * A compiled bundle: `project.index.json` and every artifact it lists, keyed
 * by the index's `artifact` paths — the output of `lute compile --all`
 * (spec 0.38.0 §4.1). The index is typed by the generated `Ir.projectIndex`
 * (spec 0.41.0 §4.4); the runtime validates the whole bundle on load.
 */
import { Schema } from "effect"
import * as Ir from "./generated/Ir.js"

export const Bundle = Schema.Struct({
  index: Ir.projectIndex,
  artifacts: Schema.Record(Schema.String, Schema.Json)
})
export type Bundle = typeof Bundle.Type
