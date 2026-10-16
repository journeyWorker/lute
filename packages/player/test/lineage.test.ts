/** Spec 0.47.0 §2.3: a second play-through survives a reload (the lineage in the profile). */
import { Effect, Schema, SubscriptionRef } from "effect"
import * as KeyValueStore from "effect/persistence/KeyValueStore"
import { describe, expect, it } from "@effect/vitest"
import { Contract } from "../../runtime/src/index.js"
import * as LuteBun from "../../runtime/src/bun.js"
import * as RuntimeSession from "../../runtime/src/Session.js"
import { compileBundle, sessionRoot } from "../../runtime/test/support.js"
import { join } from "node:path"
import { quayProject } from "./fixtures.js"
import { act, hostLayer, player, recorder, stateOf, textOf } from "./player.js"
import type { LutePlayerHandle } from "../src/LutePlayer.js"
import type { Recorder } from "./player.js"
import { ProfileStore, profileLayer } from "../src/stores/index.js"

const StoredProfile = Schema.fromJsonString(Schema.Struct({ profileVersion: Schema.String, lineage: Schema.optionalKey(Contract.Snapshot) }))

/** Run `use` over the quay fixture with one key-value store for every player it builds (a reload keeps it). */
const overQuay = <A, E, R>(use: Effect.Effect<A, E, R>) =>
  Effect.flatMap(compileBundle(quayProject), (bundle) => Effect.provide(use, hostLayer(bundle)))

/** The stored profile of `handle`'s project, as the key-value store holds it. */
const storedProfile = (handle: LutePlayerHandle) => Effect.gen(function* () {
  const raw = yield* (yield* KeyValueStore.KeyValueStore).get(`profile:${handle.session.fingerprint}`)
  if (raw === undefined) throw new Error("no profile was written")
  return Schema.decodeUnknownSync(StoredProfile)(raw)
})

/** New game, take `route` at the branch, read on to `ended`. */
const finishRun = (handle: LutePlayerHandle, route: "dock" | "hill") => Effect.gen(function* () {
  yield* act(handle, { type: "newGame" })
  yield* act(handle, { type: "choose", option: route })
  expect((yield* SubscriptionRef.get(handle.view)).flow).toBe("ended")
})

const logStart = (handle: LutePlayerHandle) => Effect.map(handle.snapshot, ({ log }) => log.map((entry) => "input" in entry ? entry.input.type : "seed" in entry ? "seed" : "snapshot"))
const shownSince = (recorded: Recorder, from: number) => recorded.lines.slice(from).map((line) => line.lineId)

describe("lineage across a reload (§2.3)", () => {
  it.effect("is written at ended, at once; New game after a reload restores it and sends newRun", () => overQuay(Effect.gen(function* () {
    const lineage = yield* Effect.scoped(Effect.gen(function* () {
      const handle = yield* player(recorder())
      yield* finishRun(handle, "hill")
      // Written at once: the test clock never runs the debounced settings write.
      const stored = yield* storedProfile(handle)
      expect(stored.lineage?.await).toEqual((yield* SubscriptionRef.get(handle.session.current)).await)
      return stored.lineage
    }))
    expect(lineage?.await.type).toBe("ended")
    yield* Effect.scoped(Effect.gen(function* () {
      const recorded = recorder()
      const handle = yield* player(recorded)
      yield* act(handle, { type: "newGame" })
      expect(yield* logStart(handle)).toEqual(["snapshot", "newRun", "raiseOccasion"])
      expect((yield* handle.snapshot).log[0]).toEqual({ snapshot: lineage })
      // The prev.run-gated line plays; user.* carried (the scene's own `+= 1` commits with the run), prev.run.* filled from run 1.
      expect(recorded.lines.map((line) => [line.lineId, textOf(line)])).toEqual([["quay.mara_0010", "Back again, I see."], ["quay.mara_0020", "The bell is ringing."]])
      expect(yield* stateOf(handle, "user.runs")).toEqual({ kind: "int", value: 1 })
      expect(yield* stateOf(handle, "prev.run.route")).toEqual({ kind: "str", value: "hill" })
    }))
  })))

  it.effect("a run abandoned through the menu does not lose the lineage", () => overQuay(Effect.scoped(Effect.gen(function* () {
    const recorded = recorder()
    const handle = yield* player(recorded)
    yield* finishRun(handle, "dock")
    // Run 2 starts from ended with newRun, and is abandoned at the branch.
    yield* act(handle, { type: "toTitle" })
    yield* act(handle, { type: "newGame" })
    expect(yield* stateOf(handle, "user.runs")).toEqual({ kind: "int", value: 1 })
    yield* act(handle, { type: "openMenu" })
    yield* act(handle, { type: "toTitle" })
    yield* act(handle, { type: "confirm", yes: true })
    expect((yield* SubscriptionRef.get(handle.view)).flow).toBe("title")
    const shown = recorded.lines.length
    yield* act(handle, { type: "newGame" })
    expect(yield* logStart(handle)).toEqual(["snapshot", "newRun", "raiseOccasion"])
    expect(shownSince(recorded, shown)).toEqual(["quay.mara_0010", "quay.mara_0020"])
    expect(yield* stateOf(handle, "prev.run.route")).toEqual({ kind: "str", value: "dock" })
    yield* act(handle, { type: "choose", option: "hill" })
    expect(yield* stateOf(handle, "user.runs")).toEqual({ kind: "int", value: 2 })
  }))))

  it.effect("a host seed is not used on New game while a lineage exists", () => overQuay(Effect.gen(function* () {
    const seed = { derive: true, state: [{ path: "user.runs", value: 40 }] }
    yield* Effect.scoped(Effect.gen(function* () {
      const handle = yield* player(recorder(), { seed })
      yield* act(handle, { type: "newGame" })
      expect(yield* logStart(handle)).toEqual(["seed", "raiseOccasion"])
      expect(yield* stateOf(handle, "user.runs")).toEqual({ kind: "int", value: 40 })
      yield* act(handle, { type: "choose", option: "dock" })
    }))
    yield* Effect.scoped(Effect.gen(function* () {
      const handle = yield* player(recorder(), { seed })
      yield* act(handle, { type: "newGame" })
      expect(yield* logStart(handle)).toEqual(["snapshot", "newRun", "raiseOccasion"])
      expect(yield* stateOf(handle, "user.runs")).toEqual({ kind: "int", value: 41 })
    }))
  })))

  it.effect("a lineage the runtime refuses is dropped and New game starts from the start point", () => Effect.gen(function* () {
    // A snapshot of another project.
    const other = yield* compileBundle(join(sessionRoot, "new-run", "project"))
    const foreign = yield* Effect.scoped(Effect.flatMap(RuntimeSession.Session, (session) => session.save).pipe(Effect.provide(LuteBun.session({ bundle: other, seed: { derive: true } }))))
    yield* overQuay(Effect.gen(function* () {
      yield* Effect.scoped(Effect.gen(function* () {
        const handle = yield* player(recorder())
        // Written by the profile store itself, in the format version it writes.
        yield* Effect.flatMap(ProfileStore, (profile) => profile.setLineage(foreign)).pipe(Effect.provide(profileLayer({ project: handle.session.fingerprint })))
        expect((yield* storedProfile(handle)).lineage?.project).toBe(foreign.project)
      }))
      yield* Effect.scoped(Effect.gen(function* () {
        const recorded = recorder()
        const handle = yield* player(recorded)
        yield* act(handle, { type: "newGame" })
        expect(yield* logStart(handle)).toEqual(["seed", "raiseOccasion"])
        expect(recorded.lines.map((line) => line.lineId)).toEqual(["quay.mara_0020"])
        expect(yield* stateOf(handle, "user.runs")).toEqual({ kind: "int", value: 0 })
        expect((yield* storedProfile(handle)).lineage).toBeUndefined()
      }))
    }))
  }))

  it.effect("New game on a session restored from an ended snapshot sends newRun (never `started` again)", () => overQuay(Effect.gen(function* () {
    const ended = yield* Effect.scoped(Effect.gen(function* () {
      const handle = yield* player(recorder())
      yield* finishRun(handle, "hill")
      return (yield* handle.snapshot).snapshot
    }))
    yield* Effect.scoped(Effect.gen(function* () {
      const recorded = recorder()
      const handle = yield* player(recorded, { snapshot: ended })
      yield* act(handle, { type: "newGame" })
      expect(yield* logStart(handle)).toEqual(["snapshot", "newRun", "raiseOccasion"])
      expect(recorded.lines.map((line) => line.lineId)).toEqual(["quay.mara_0010", "quay.mara_0020"])
      expect(yield* stateOf(handle, "prev.run.route")).toEqual({ kind: "str", value: "hill" })
    }))
  })))
})
