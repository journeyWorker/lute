// spec 0.47.0 §2.1/§6: the ended resolver reads the view the real runtime
// returns. The demo is compiled with the lute binary and played through
// `@lute-lang/runtime/bun` to each `run.path` ending.
import { readFileSync } from "node:fs"
import { join } from "node:path"
import { Context, Effect, Exit, Layer, Scope, SubscriptionRef } from "effect"
import { afterAll, beforeAll, describe, expect, it } from "vitest"
import { BindingsLoader, type Bindings } from "@lute-lang/player"
import { Contract, Session, type Program } from "@lute-lang/runtime"
import * as LuteBun from "@lute-lang/runtime/bun"
import { resolveEndedScreen, type EndedBinding } from "../../src/ui/ended.js"
import { compileProject, demoProject } from "../support/bundle.js"

const screen = { _tag: "Ended" as const, reason: "terminal" as const }

interface Ended {
  readonly program: Program.Program
  readonly state: Program.RuntimeState
  readonly view: Contract.WorldView
}

let bundle = ""
let bindings: Bindings
// Programs are valid for their session's scope: the endings stay open until the suite ends.
const scope = Scope.makeUnsafe()

/**
 * Raise `chapter` at idle and answer every menu with the first open option
 * of `prefer`, else its first open option, until the run ends.
 */
const playToEnd = (prefer: ReadonlyArray<string>): Effect.Effect<Ended, unknown> => Effect.gen(function* () {
  const services = yield* Layer.buildWithScope(LuteBun.session({ bundle, seed: { derive: true } }), scope)
  const session = Context.get(services, Session.Session)
  let output = session.started
  for (let step = 0; step < 200 && output.await.type !== "ended"; step++) {
    const awaiting = output.await
    if (awaiting.type === "idle") {
      output = yield* session.send({ type: "raiseOccasion", occasion: "chapter" })
    } else if (awaiting.type === "awaitChoice") {
      const open = awaiting.menu.options.filter((option) => option.verdict === "open")
      const option = open.find((candidate) => prefer.includes(candidate.id)) ?? open[0]
      if (option === undefined) throw new Error(`no open option in ${awaiting.menu.id}`)
      output = yield* session.send({ type: "choose", option: option.id, request: awaiting.request })
    } else {
      throw new Error(`unexpected await ${awaiting.type}`)
    }
  }
  expect(output.await.type).toBe("ended")
  const { state } = yield* SubscriptionRef.get(session.current)
  return { program: session.program, state, view: yield* session.program.view(state) }
})

const ended = (binding: EndedBinding | null | undefined, run: Ended) =>
  Effect.runPromise(resolveEndedScreen(run.program, run.state, binding, screen))

describe("resolveEndedScreen over the real runtime view", () => {
  let dawn: Ended
  let dusk: Ended

  beforeAll(async () => {
    bundle = compileProject("demo-node", demoProject)
    bindings = await Effect.runPromise(BindingsLoader.decode(readFileSync(join(demoProject, "player.bindings.yaml"), "utf8")))
    dawn = await Effect.runPromise(playToEnd(["release"]))
    dusk = await Effect.runPromise(playToEnd(["hold"]))
  }, 240_000)

  afterAll(() => Effect.runPromise(Scope.close(scope, Exit.void)))

  it("reads a closed-domain member as a str Value entry", () => {
    expect(dawn.view.state["run.path"]).toEqual({ kind: "str", value: "dawn" })
    expect(dusk.view.state["run.path"]).toEqual({ kind: "str", value: "dusk" })
  })

  it.each([["dawn", () => dawn], ["dusk", () => dusk]] as const)("selects the demo's %s screen", async (member, run) => {
    const binding = bindings.ui?.ended
    const expected = binding?.screens[member]
    expect(expected).toBeDefined()
    await expect(ended(binding, run())).resolves.toEqual({ ...screen, ...expected })
  })

  it("shows the stock screen for an unmapped member, another kind, an absent entry or no binding", async () => {
    const onlyDawn: EndedBinding = { by: "run.path", screens: { dawn: { title: "Dawn", text: "" } } }
    await expect(ended(onlyDawn, dusk)).resolves.toBeUndefined()
    // run.trust is an int, run.finished a bool: never a screen, even when a key matches its rendering.
    expect(dawn.view.state["run.trust"]).toEqual({ kind: "int", value: 2 })
    await expect(ended({ by: "run.trust", screens: { "2": { title: "Trust", text: "" } } }, dawn)).resolves.toBeUndefined()
    await expect(ended({ by: "run.finished", screens: { true: { title: "Finished", text: "" } } }, dawn)).resolves.toBeUndefined()
    await expect(ended({ by: "run.missing", screens: { dawn: { title: "Dawn", text: "" } } }, dawn)).resolves.toBeUndefined()
    await expect(ended(undefined, dawn)).resolves.toBeUndefined()
    await expect(ended(null, dawn)).resolves.toBeUndefined()
  })

  it("shows the stock screen when the query fails", async () => {
    const broken = { ...dawn, state: { corrupt: true } as unknown as Program.RuntimeState }
    await expect(ended(bindings.ui?.ended, broken)).resolves.toBeUndefined()
  })
})
