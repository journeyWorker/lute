import { createActor } from "xstate"
import { describe, expect, it } from "vitest"
import type { MachineEvent } from "../../src/model.js"
import { derivePlayerView, playerMachine } from "../../src/machine/playerMachine.js"

const choiceAwait = (timeout?: number) => ({
  type: "awaitChoice" as const,
  request: 7,
  menu: {
    construct: "menu",
    document: "doc",
    id: "choice",
    options: [],
    position: "0",
    presentation: 0,
    ...(timeout === undefined ? {} : { timeout }),
  },
})

const bridgeAwait = {
  type: "awaitBridge" as const,
  document: "doc",
  fields: {},
  position: "0",
  request: 4,
  tag: "host",
}

const makeActor = () => {
  const actor = createActor(playerMachine)
  const commands: Array<{ readonly type: string }> = []
  actor.on("*", (command) => commands.push(command))
  actor.start()
  return { actor, commands }
}

const playingActor = () => {
  const run = makeActor()
  run.actor.send({ type: "boot.ready", canContinue: true })
  run.actor.send({ type: "newGame" })
  run.actor.send({ type: "batch.started" })
  return run
}

const lineActor = () => {
  const run = playingActor()
  run.actor.send({ type: "line.typing", read: false, hasVoice: false })
  return run
}

const commandTypes = (commands: ReadonlyArray<{ readonly type: string }>) => commands.map(({ type }) => type)

describe("playerMachine", () => {
  it("reveals typing lines and releases shown lines on advance", () => {
    const run = lineActor()
    run.actor.send({ type: "advance" })
    expect(run.actor.getSnapshot().value).toMatchObject({ story: "shown" })
    expect(run.commands.at(-1)?.type).toBe("line.reveal")
    run.actor.send({ type: "advance" })
    expect(run.commands.at(-1)?.type).toBe("line.release")
    expect(run.actor.getSnapshot().value).toMatchObject({ story: "running" })
  })

  it("ignores advance while an overlay or choice is open", () => {
    const overlay = lineActor()
    overlay.actor.send({ type: "openMenu" })
    const beforeOverlay = overlay.commands.length
    overlay.actor.send({ type: "advance" })
    expect(overlay.commands).toHaveLength(beforeOverlay)
    const choice = playingActor()
    choice.actor.send({ type: "batch.settled", await: choiceAwait() })
    const beforeChoice = choice.commands.length
    choice.actor.send({ type: "advance" })
    expect(choice.commands).toHaveLength(beforeChoice)
  })

  it("starts auto with voice", () => {
    const run = playingActor()
    run.actor.send({ type: "toggleAuto" })
    run.actor.send({ type: "line.shown", read: true, hasVoice: true })
    expect(run.commands.at(-1)).toEqual({ type: "auto.start", waitVoice: true })
  })

  it("starts auto without voice", () => {
    const run = playingActor()
    run.actor.send({ type: "toggleAuto" })
    run.actor.send({ type: "line.shown", read: true, hasVoice: false })
    expect(run.commands.at(-1)).toEqual({ type: "auto.start", waitVoice: false })
  })

  it("releases a line when auto elapses", () => {
    const run = playingActor()
    run.actor.send({ type: "toggleAuto" })
    run.actor.send({ type: "line.shown", read: true, hasVoice: true })
    run.actor.send({ type: "auto.elapsed" })
    expect(run.commands.at(-1)?.type).toBe("line.release")
  })

  it("cancels pending auto on reader input and rearms on the next line", () => {
    const run = playingActor()
    run.actor.send({ type: "toggleAuto" })
    run.actor.send({ type: "line.shown", read: true, hasVoice: true })
    run.actor.send({ type: "advance" })
    expect(commandTypes(run.commands)).toContain("auto.cancel")
    run.actor.send({ type: "line.shown", read: true, hasVoice: false })
    expect(run.commands.at(-1)).toEqual({ type: "auto.start", waitVoice: false })
  })

  it("never answers a choice in auto mode", () => {
    const run = playingActor()
    run.actor.send({ type: "toggleAuto" })
    run.actor.send({ type: "batch.settled", await: choiceAwait() })
    run.actor.send({ type: "auto.elapsed" })
    expect(commandTypes(run.commands)).not.toContain("session.send")
  })

  it("read-only skip releases read lines and leaves skip on unread lines", () => {
    const run = playingActor()
    run.actor.send({ type: "toggleSkip" })
    run.actor.send({ type: "line.shown", read: true, hasVoice: false })
    expect(run.commands.at(-1)?.type).toBe("line.release")
    run.actor.send({ type: "line.shown", read: false, hasVoice: false })
    expect(run.actor.getSnapshot().context.mode).toBe("normal")
  })

  it("does not activate read-only skip on an unread current line", () => {
    const run = lineActor()
    const before = run.commands.length
    run.actor.send({ type: "toggleSkip" })
    expect(run.actor.getSnapshot().context.mode).toBe("normal")
    expect(run.commands.slice(before)).toHaveLength(0)
  })
  it("stops read skip at choices", () => {
    const run = playingActor()
    run.actor.send({ type: "toggleSkip" })
    run.actor.send({ type: "batch.settled", await: choiceAwait() })
    expect(run.actor.getSnapshot().context.mode).toBe("normal")
  })

  it("stops read skip at bridges", () => {
    const run = playingActor()
    run.actor.send({ type: "toggleSkip" })
    run.actor.send({ type: "batch.settled", await: bridgeAwait })
    expect(run.actor.getSnapshot().context.mode).toBe("normal")
  })

  it("stops read skip at idle", () => {
    const run = playingActor()
    run.actor.send({ type: "toggleSkip" })
    run.actor.send({ type: "batch.settled", await: { type: "idle" } })
    expect(run.actor.getSnapshot().context.mode).toBe("normal")
  })

  it("stops read skip at ended", () => {
    const run = playingActor()
    run.actor.send({ type: "toggleSkip" })
    run.actor.send({ type: "batch.settled", await: { type: "ended", reason: "done" } })
    expect(run.actor.getSnapshot().context.mode).toBe("normal")
  })

  it("skip all releases every line", () => {
    const run = playingActor()
    run.actor.send({ type: "setSettings", settings: { skipMode: "all" } })
    run.actor.send({ type: "toggleSkip" })
    run.actor.send({ type: "line.shown", read: false, hasVoice: false })
    expect(run.commands.at(-1)?.type).toBe("line.release")
  })

  it("skip all releases an unread current line on activation", () => {
    const run = lineActor()
    run.actor.send({ type: "setSettings", settings: { skipMode: "all" } })
    run.actor.send({ type: "toggleSkip" })
    expect(run.actor.getSnapshot().context.mode).toBe("skip")
    expect(run.commands.at(-1)?.type).toBe("line.release")
  })
  it("skipHold is active only while held", () => {
    const run = playingActor()
    run.actor.send({ type: "skipHold", held: true })
    expect(run.actor.getSnapshot().context.mode).toBe("skip")
    run.actor.send({ type: "skipHold", held: false })
    expect(run.actor.getSnapshot().context.mode).toBe("normal")
  })

  it("shows an open choice", () => {
    const run = playingActor()
    run.actor.send({ type: "batch.settled", await: choiceAwait() })
    expect(run.commands.at(-1)).toEqual({ type: "choice.show", await: choiceAwait() })
  })

  it("starts countdown for timed choices", () => {
    const run = playingActor()
    run.actor.send({ type: "batch.settled", await: choiceAwait(9) })
    expect(commandTypes(run.commands).slice(-2)).toEqual(["choice.show", "countdown.start"])
  })

  it("chooses with cancel, hide, and runtime input", () => {
    const run = playingActor()
    run.actor.send({ type: "batch.settled", await: choiceAwait(9) })
    run.actor.send({ type: "choose", option: "yes" })
    expect(run.commands.slice(-3)).toEqual([
      { type: "countdown.cancel" },
      { type: "choice.hide" },
      { type: "session.send", input: { type: "choose", option: "yes", request: 7 } },
    ])
  })

  it("expires timed choices with timeout and request", () => {
    const run = playingActor()
    run.actor.send({ type: "batch.settled", await: choiceAwait(9) })
    run.actor.send({ type: "countdown.expired" })
    expect(run.commands.slice(-3)).toEqual([
      { type: "countdown.cancel" },
      { type: "choice.hide" },
      { type: "session.send", input: { type: "timeout", request: 7 } },
    ])
  })

  it("pauses countdown when an overlay opens and resumes on last pop", () => {
    const run = playingActor()
    run.actor.send({ type: "batch.settled", await: choiceAwait(3) })
    run.actor.send({ type: "openMenu" })
    run.actor.send({ type: "close" })
    expect(commandTypes(run.commands).slice(-2)).toEqual(["countdown.pause", "countdown.resume"])
  })

  it("cancels pending auto when an overlay opens", () => {
    const run = playingActor()
    run.actor.send({ type: "toggleAuto" })
    run.actor.send({ type: "line.shown", read: true, hasVoice: false })
    run.actor.send({ type: "openMenu" })
    expect(commandTypes(run.commands)).toContain("auto.cancel")
  })

  it("calls a bridge", () => {
    const run = playingActor()
    run.actor.send({ type: "batch.settled", await: bridgeAwait })
    expect(run.commands.at(-1)).toEqual({ type: "bridge.call", await: bridgeAwait })
  })

  it("continues after a bridge answer", () => {
    const run = playingActor()
    run.actor.send({ type: "batch.settled", await: bridgeAwait })
    run.actor.send({ type: "bridge.answered" })
    expect(run.actor.getSnapshot().value).toMatchObject({ story: "running" })
  })

  it("halts on an unhandled bridge", () => {
    const run = playingActor()
    run.actor.send({ type: "batch.settled", await: bridgeAwait })
    run.actor.send({ type: "bridge.unhandled", tag: "missing" })
    expect(run.actor.getSnapshot().value).toMatchObject({ story: "halted" })
  })

  it("flows idle", () => {
    const run = playingActor()
    run.actor.send({ type: "batch.settled", await: { type: "idle" } })
    expect(run.commands.at(-1)).toEqual({ type: "idle.flow", await: { type: "idle" } })
  })

  it("shows the ended screen", () => {
    const run = playingActor()
    run.actor.send({ type: "batch.settled", await: { type: "ended", reason: "done" } })
    expect(run.commands.at(-1)).toEqual({ type: "screen.show", screen: { _tag: "Ended", reason: "done" } })
  })

  it("shows the halted screen", () => {
    const run = playingActor()
    run.actor.send({ type: "batch.settled", await: { type: "halted", kind: "error", message: "bad", site: null } })
    expect(run.commands.at(-1)).toEqual({ type: "screen.show", screen: { _tag: "Halted", message: "bad" } })
  })

  it("requires confirmation before returning to title", () => {
    const run = playingActor()
    run.actor.send({ type: "batch.settled", await: { type: "ended", reason: "done" } })
    run.actor.send({ type: "toTitle" })
    expect(run.actor.getSnapshot().context.pendingConfirm).toEqual({ type: "toTitle" })
    run.actor.send({ type: "confirm", yes: true })
    expect(run.actor.getSnapshot().value).toMatchObject({ flow: "title", story: "running" })
  })

  it("hides UI and consumes the first input while hidden", () => {
    const run = playingActor()
    run.actor.send({ type: "hideUI" })
    expect(run.actor.getSnapshot().context.uiHidden).toBe(true)
    const before = run.commands.length
    run.actor.send({ type: "advance" })
    expect(run.actor.getSnapshot().context.uiHidden).toBe(false)
    expect(run.commands).toHaveLength(before)
    expect(run.commands.at(-1)?.type).toBe("ui.hidden")
  })

  it("quick-saves while playing", () => {
    const run = playingActor()
    run.actor.send({ type: "quickSave" })
    expect(run.commands.at(-1)).toEqual({ type: "save", slot: "quick" })
  })

  it("confirms quick-load while playing", () => {
    const run = playingActor()
    run.actor.send({ type: "quickLoad" })
    expect(run.actor.getSnapshot().context.pendingConfirm).toEqual({ type: "load", slot: "quick" })
  })

  it("confirms overwrite of an occupied save slot", () => {
    const run = playingActor()
    run.actor.send({ type: "openSave" })
    run.actor.send({ type: "slot.occupied", slot: 2, occupied: true })
    run.actor.send({ type: "saveSlot", slot: 2 })
    expect(run.actor.getSnapshot().context.pendingConfirm).toEqual({ type: "overwrite", slot: 2 })
  })

  it("confirms loading a save slot", () => {
    const run = playingActor()
    run.actor.send({ type: "openLoad" })
    run.actor.send({ type: "loadSlot", slot: 2 })
    expect(run.actor.getSnapshot().context.pendingConfirm).toEqual({ type: "load", slot: 2 })
  })

  it("confirms deleting a save slot", () => {
    const run = playingActor()
    run.actor.send({ type: "openLoad" })
    run.actor.send({ type: "deleteSlot", slot: 2 })
    expect(run.actor.getSnapshot().context.pendingConfirm).toEqual({ type: "delete", slot: 2 })
  })

  it("load.done resets await, mode, and overlays", () => {
    const run = playingActor()
    run.actor.send({ type: "toggleSkip" })
    run.actor.send({ type: "openMenu" })
    run.actor.send({ type: "load.done", await: { type: "idle" } })
    expect(run.actor.getSnapshot().value).toMatchObject({ mode: "normal", overlay: "none", story: "idle" })
  })

  it("rolls back from a line", () => {
    const run = lineActor()
    run.actor.send({ type: "rollback" })
    expect(run.commands.at(-1)?.type).toBe("rollback")
  })

  it("rolls back from a choice", () => {
    const run = playingActor()
    run.actor.send({ type: "batch.settled", await: choiceAwait() })
    run.actor.send({ type: "rollback" })
    expect(run.commands.at(-1)?.type).toBe("rollback")
  })

  it("rolls back from a later choice", () => {
    const run = playingActor()
    run.actor.send({ type: "batch.settled", await: choiceAwait() })
    run.actor.send({ type: "choose", option: "first" })
    run.actor.send({ type: "batch.settled", await: choiceAwait() })
    run.actor.send({ type: "rollback" })
    expect(run.commands.at(-1)?.type).toBe("rollback")
  })
  it("rolls back from idle", () => {
    const run = playingActor()
    run.actor.send({ type: "batch.settled", await: { type: "idle" } })
    run.actor.send({ type: "rollback" })
    expect(run.commands.at(-1)?.type).toBe("rollback")
  })

  it("does not roll back during a running batch", () => {
    const run = playingActor()
    const before = run.commands.length
    run.actor.send({ type: "rollback" })
    expect(run.commands).toHaveLength(before)
  })

  it("boot.ready enters title", () => {
    const run = makeActor()
    run.actor.send({ type: "boot.ready", canContinue: true })
    expect(run.actor.getSnapshot().value).toMatchObject({ flow: "title" })
  })

  it("opens settings from title", () => {
    const run = makeActor()
    run.actor.send({ type: "boot.ready", canContinue: true })
    run.actor.send({ type: "openSettings" })
    expect(run.actor.getSnapshot().value).toMatchObject({ overlay: "settings" })
  })

  it("starts a new game", () => {
    const run = makeActor()
    run.actor.send({ type: "boot.ready", canContinue: false })
    run.actor.send({ type: "newGame" })
    expect(run.commands.at(-1)?.type).toBe("session.newGame")
  })

  it("continues a saved game", () => {
    const run = makeActor()
    run.actor.send({ type: "boot.ready", canContinue: true })
    run.actor.send({ type: "continue" })
    expect(run.commands.at(-1)?.type).toBe("session.continue")
  })

  it("emits mode.changed entering auto", () => {
    const run = playingActor()
    run.actor.send({ type: "toggleAuto" })
    expect(run.commands.at(-1)).toEqual({ type: "mode.changed", mode: "auto" })
  })

  it("emits mode.changed leaving auto", () => {
    const run = playingActor()
    run.actor.send({ type: "toggleAuto" })
    run.actor.send({ type: "toggleAuto" })
    expect(run.commands.at(-1)).toEqual({ type: "mode.changed", mode: "normal" })
  })

  it("emits mode.changed entering and leaving skip", () => {
    const run = playingActor()
    run.actor.send({ type: "toggleSkip" })
    expect(run.commands.at(-1)).toEqual({ type: "mode.changed", mode: "skip" })
    run.actor.send({ type: "toggleSkip" })
    expect(run.commands.at(-1)).toEqual({ type: "mode.changed", mode: "normal" })
  })

  it("emits mode.changed entering and leaving skipHold", () => {
    const run = playingActor()
    run.actor.send({ type: "skipHold", held: true })
    expect(run.commands.at(-1)).toEqual({ type: "mode.changed", mode: "skip" })
    run.actor.send({ type: "skipHold", held: false })
    expect(run.commands.at(-1)).toEqual({ type: "mode.changed", mode: "normal" })
  })

  it("derives a renderer view from a snapshot", () => {
    const run = playingActor()
    run.actor.send({ type: "openMenu" })
    const view = derivePlayerView(run.actor.getSnapshot())
    expect(view).toEqual({ flow: "playing", story: "running", mode: "normal", overlay: "menu", countdownRemaining: undefined, uiHidden: false })
  })

  it("derives choice from the nested story state", () => {
    const run = playingActor()
    run.actor.send({ type: "batch.settled", await: choiceAwait() })
    expect(derivePlayerView(run.actor.getSnapshot()).story).toBe("choice")
  })
  it("survives seeded random action sequences without sending runtime input through overlays", () => {
    let seed = 0x12345678
    const next = () => {
      seed = (seed * 1664525 + 1013904223) >>> 0
      return seed
    }
    const actions: ReadonlyArray<MachineEvent> = [
      { type: "advance" }, { type: "toggleAuto" }, { type: "toggleSkip" }, { type: "skipHold", held: true },
      { type: "skipHold", held: false }, { type: "rollback" }, { type: "openMenu" }, { type: "openBacklog" },
      { type: "openSave" }, { type: "openLoad" }, { type: "openSettings" }, { type: "quickSave" },
      { type: "quickLoad" }, { type: "hideUI" }, { type: "choose", option: "x" }, { type: "close" },
      { type: "newGame" }, { type: "continue" }, { type: "toTitle" }, { type: "saveSlot", slot: 1 },
      { type: "loadSlot", slot: 1 }, { type: "deleteSlot", slot: 1 }, { type: "confirm", yes: true },
    ]
    for (let sequence = 0; sequence < 500; sequence += 1) {
      const run = makeActor()
      for (let step = 0; step < 50; step += 1) {
        const event = actions[next() % actions.length]
        const open = run.actor.getSnapshot().context.overlayStack.length > 0
        const before = run.commands.length
        if (event === undefined) continue
        expect(() => run.actor.send(event)).not.toThrow()
        if (open) expect(run.commands.slice(before).some((command) => command.type === "session.send")).toBe(false)
      }
    }
  })
})
