/**
 * The typed failures of the runtime (spec 0.39.0 §5.3).
 *
 * A rejected input is an expected failure: each runtime code is its own
 * tagged error, carrying the runtime's `code` and `message`. A code this
 * package does not know is contract drift — a defect, never a typed error.
 */
import { Effect, Schema } from "effect"
import type { Rejected } from "./generated/Contract.js"

/** `E-RUNTIME-BUSY`: the current await does not accept this input. */
export class RuntimeBusy extends Schema.TaggedError<RuntimeBusy>()("RuntimeBusy", {
  code: Schema.Literal("E-RUNTIME-BUSY"),
  message: Schema.String
}) {}

/** `E-RUNTIME-REQUEST`: an answer names another request than the pending await. */
export class RequestMismatch extends Schema.TaggedError<RequestMismatch>()("RequestMismatch", {
  code: Schema.Literal("E-RUNTIME-REQUEST"),
  message: Schema.String
}) {}

/** `E-RUNTIME-OPTION`: the chosen option is absent from the menu or not open. */
export class OptionUnavailable extends Schema.TaggedError<OptionUnavailable>()("OptionUnavailable", {
  code: Schema.Literal("E-RUNTIME-OPTION"),
  message: Schema.String
}) {}

/** `E-RUNTIME-BRIDGE-SHAPE`: a bridge result misses or mistypes a read field. */
export class BridgeShapeMismatch extends Schema.TaggedError<BridgeShapeMismatch>()("BridgeShapeMismatch", {
  code: Schema.Literal("E-RUNTIME-BRIDGE-SHAPE"),
  message: Schema.String
}) {}

/** `E-RUNTIME-INPUT`: a seed or input names something the project lacks. */
export class InvalidInput extends Schema.TaggedError<InvalidInput>()("InvalidInput", {
  code: Schema.Literal("E-RUNTIME-INPUT"),
  message: Schema.String
}) {}

/** `E-RUNTIME-HALTED`: the state halted; restore an earlier snapshot. */
export class RuntimeHalted extends Schema.TaggedError<RuntimeHalted>()("RuntimeHalted", {
  code: Schema.Literal("E-RUNTIME-HALTED"),
  message: Schema.String
}) {}

/** `E-RUNTIME-IR-VERSION`: the bundle is not one this runtime executes. */
export class IrVersionMismatch extends Schema.TaggedError<IrVersionMismatch>()("IrVersionMismatch", {
  code: Schema.Literal("E-RUNTIME-IR-VERSION"),
  message: Schema.String
}) {}

/** `E-RUNTIME-SNAPSHOT-PROJECT`: the snapshot belongs to another bundle. */
export class SnapshotProjectMismatch extends Schema.TaggedError<SnapshotProjectMismatch>()(
  "SnapshotProjectMismatch",
  { code: Schema.Literal("E-RUNTIME-SNAPSHOT-PROJECT"), message: Schema.String }
) {}

/** `E-RUNTIME-SNAPSHOT-VERSION`: the snapshot is of another Event minor. */
export class SnapshotVersionMismatch extends Schema.TaggedError<SnapshotVersionMismatch>()(
  "SnapshotVersionMismatch",
  { code: Schema.Literal("E-RUNTIME-SNAPSHOT-VERSION"), message: Schema.String }
) {}

/** Every rejection the runtime returns. */
export type RuntimeRejection =
  | RuntimeBusy
  | RequestMismatch
  | OptionUnavailable
  | BridgeShapeMismatch
  | InvalidInput
  | RuntimeHalted
  | IrVersionMismatch
  | SnapshotProjectMismatch
  | SnapshotVersionMismatch

const byCode: Readonly<Record<string, (message: string) => RuntimeRejection>> = {
  "E-RUNTIME-BUSY": (message) => new RuntimeBusy({ code: "E-RUNTIME-BUSY", message }),
  "E-RUNTIME-REQUEST": (message) => new RequestMismatch({ code: "E-RUNTIME-REQUEST", message }),
  "E-RUNTIME-OPTION": (message) => new OptionUnavailable({ code: "E-RUNTIME-OPTION", message }),
  "E-RUNTIME-BRIDGE-SHAPE": (message) =>
    new BridgeShapeMismatch({ code: "E-RUNTIME-BRIDGE-SHAPE", message }),
  "E-RUNTIME-INPUT": (message) => new InvalidInput({ code: "E-RUNTIME-INPUT", message }),
  "E-RUNTIME-HALTED": (message) => new RuntimeHalted({ code: "E-RUNTIME-HALTED", message }),
  "E-RUNTIME-IR-VERSION": (message) => new IrVersionMismatch({ code: "E-RUNTIME-IR-VERSION", message }),
  "E-RUNTIME-SNAPSHOT-PROJECT": (message) =>
    new SnapshotProjectMismatch({ code: "E-RUNTIME-SNAPSHOT-PROJECT", message }),
  "E-RUNTIME-SNAPSHOT-VERSION": (message) =>
    new SnapshotVersionMismatch({ code: "E-RUNTIME-SNAPSHOT-VERSION", message })
}

/**
 * Fail with the typed error of a runtime rejection; a code this package does
 * not know dies.
 */
export const fromRejected = (rejected: Rejected): Effect.Effect<never, RuntimeRejection> => {
  const make = byCode[rejected.code]
  return make === undefined
    ? Effect.die(new Error(`unknown runtime rejection ${rejected.code}: ${rejected.message}`))
    : Effect.fail(make(rejected.message))
}

/** The bundle's `project.index.json` or an artifact it lists does not exist. */
export class BundleNotFound extends Schema.TaggedError<BundleNotFound>()("BundleNotFound", {
  path: Schema.String,
  message: Schema.String
}) {}

/** The bundle exists but could not be read. */
export class BundleUnreadable extends Schema.TaggedError<BundleUnreadable>()("BundleUnreadable", {
  path: Schema.String,
  message: Schema.String
}) {}

/** A bundle file is not the JSON a bundle holds. */
export class BundleMalformed extends Schema.TaggedError<BundleMalformed>()("BundleMalformed", {
  path: Schema.String,
  message: Schema.String
}) {}

/** Why a bundle could not be loaded. */
export type BundleError = BundleNotFound | BundleUnreadable | BundleMalformed

/** The wasm binding could not be fetched or instantiated. */
export class WasmLoadError extends Schema.TaggedError<WasmLoadError>()("WasmLoadError", {
  message: Schema.String
}) {}

/** The input log could not be read or written. */
export class InputLogError extends Schema.TaggedError<InputLogError>()("InputLogError", {
  message: Schema.String
}) {}
