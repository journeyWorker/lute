import { Context, Deferred, Effect, Layer, Stream } from "effect"
import type { Container } from "pixi.js"
import type { Behaviour, Running, WorldShape } from "./types.js"
import { PixiApp } from "./app.js"

const complete = (entry: Running): void => {
  Effect.runSync(Deferred.succeed(entry.done, undefined))
}

/** The scene behaviour scheduler. */
export class World extends Context.Service<World, WorldShape>()("Lute.Web.World") {
  /** Build a world and run it from the Pixi application ticker. */
  static readonly layer = Layer.effect(
    World,
    Effect.gen(function* () {
      const pixi = yield* PixiApp
      const entries: Running[] = []
      const world: WorldShape = {
        run: (target: Container, behaviour: Behaviour) => {
          const done = Deferred.makeUnsafe<void>()
          entries.push({ target, behaviour, done })
          return Deferred.await(done)
        },
        finishAll: (target) =>
          Effect.sync(() => {
            let index = entries.length - 1
            while (index >= 0) {
              const entry = entries[index]
              if (entry === undefined) {
                index -= 1
                continue
              }
              if (target === undefined || entry.target === target) {
                entry.behaviour.finish()
                entry.behaviour.dispose()
                complete(entry)
                const last = entries.pop()
                if (last !== undefined && index < entries.length) entries[index] = last
              }
              index -= 1
            }
          }),
        cancel: (target) =>
          Effect.sync(() => {
            let index = entries.length - 1
            while (index >= 0) {
              const entry = entries[index]
              if (entry === undefined) {
                index -= 1
                continue
              }
              if (target === undefined || entry.target === target) {
                entry.behaviour.dispose()
                complete(entry)
                const last = entries.pop()
                if (last !== undefined && index < entries.length) entries[index] = last
              }
              index -= 1
            }
          }),
        size: Effect.sync(() => entries.length)
      }
      const ticker = Stream.runForEach(pixi.frames, (dt) => Effect.sync(() => {
        let index = entries.length - 1
        while (index >= 0) {
          const entry = entries[index]
          if (entry === undefined) {
            index -= 1
            continue
          }
          if (entry.behaviour.update(dt) === "done") {
            entry.behaviour.dispose()
            complete(entry)
            const last = entries.pop()
            if (last !== undefined && index < entries.length) entries[index] = last
          }
          index -= 1
        }
      }))
      yield* Effect.forkScoped(ticker)
      return world
    })
  )
}
