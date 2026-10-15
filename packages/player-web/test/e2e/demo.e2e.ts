import { createServer } from "node:net"
import { existsSync, mkdtempSync, rmSync, writeFileSync } from "node:fs"
import { join } from "node:path"
import { tmpdir } from "node:os"
import { spawn, type ChildProcessWithoutNullStreams } from "node:child_process"
import { chromium, expect, test, type Browser, type Page } from "playwright/test"

interface DemoWindow extends Window {
  luteDemo: {
    view: () => Promise<{ flow: string; story: string; countdownRemaining?: number }>
    act: (action: unknown) => Promise<void>
  }
}

test.describe.serial("player demo", () => {
  let browser: Browser
  let page: Page
  let mountPage: Page
  let server: ChildProcessWithoutNullStreams
  let baseUrl = ""
  let mountScript = ""
  const errors: string[] = []

  test.beforeAll(async () => {
    const port = await new Promise<number>((resolvePort, reject) => {
      const socket = createServer()
      socket.once("error", reject)
      socket.listen(0, "127.0.0.1", () => {
        const address = socket.address()
        socket.close(() => resolvePort(typeof address === "object" && address !== null ? address.port : 0))
      })
    })
    const repo = process.cwd().replace(/\/packages\/player-web$/, "")
    const release = `${repo}/target/release/lute`
    const debug = `${repo}/target/debug/lute`
    const env = { ...process.env, PORT: String(port), ...(existsSync(release) ? { LUTE_BIN: release } : existsSync(debug) ? { LUTE_BIN: debug } : {}) }
    server = spawn("bun", ["scripts/demo.ts"], { cwd: `${repo}/packages/player-web`, env })
    await new Promise<void>((resolveServer, rejectServer) => {
      let output = ""
      server.stdout.on("data", (chunk: Buffer) => {
        output += chunk.toString()
        const match = output.match(/Lute player demo: (https?:\/\/[^\s]+)/)
        if (match !== null) {
          baseUrl = match[1]!
          resolveServer()
        }
      })
      server.stderr.on("data", (chunk: Buffer) => output += chunk.toString())
      server.once("exit", (code) => rejectServer(new Error(`demo exited ${code}: ${output}`)))
    })
    const hostDir = mkdtempSync(join(`${repo}/packages/player-web`, ".mount-parity-"))
    const hostFile = join(hostDir, "main.ts")
    writeFileSync(hostFile, `
import { Effect, Exit, Scope, SubscriptionRef } from "effect"
import { Assets } from "pixi.js"
import { mount, mountPlayer } from ${JSON.stringify(`${repo}/packages/player-web/src/index.ts`)}

const options = {
  bindings: { url: ${JSON.stringify(`${baseUrl}player.bindings.yaml`)} },
  bundleUrl: ${JSON.stringify(`${baseUrl}bundle/`)},
  wasmUrl: ${JSON.stringify(`${baseUrl}wasm/lute_runtime_wasm_bg.wasm`)},
  assetsUrl: ${JSON.stringify(`${baseUrl}assets/`)}
} as const
const first = document.querySelector("#first")
const second = document.querySelector("#second")
if (!(first instanceof HTMLElement) || !(second instanceof HTMLElement)) throw new Error("mount parity hosts missing")
const scope = Scope.makeUnsafe("parallel")
const handle = await Effect.runPromise(Effect.provideService(mount(first, options), Scope.Scope, scope))
const facade = await mountPlayer(second, options)
const assetWarnings: string[] = []
const previousWarn = console.warn
console.warn = (...args: unknown[]) => {
  assetWarnings.push(args.map(String).join(" "))
  previousWarn(...args)
}
await Assets.init({ basePath: "forge/" })
console.warn = previousWarn
const describe = (value: unknown): unknown => {
  if (value !== null && typeof value === "object" && "_tag" in value) {
    const error = value as { readonly _tag?: unknown; readonly message?: unknown }
    return { _tag: error._tag, message: error.message }
  }
  return String(value)
}
const effect = {
  act: (action: unknown) => Effect.runPromise(handle.actions(action as never)),
  view: () => Effect.runPromise(SubscriptionRef.get(handle.view)),
  snapshot: () => Effect.runPromise(handle.snapshot),
  send: (input: unknown) => Effect.runPromise(handle.send(input as never)),
  restore: (save: unknown) => Effect.runPromise(handle.restore(save as never)),
  bundleInfo: { index: handle.bundle.index, fingerprint: handle.program.fingerprint },
  error: describe
}
const closeEffect = async (): Promise<void> => {
  await Effect.runPromise(handle.shutdown)
  await Effect.runPromise(Scope.close(scope, Exit.void))
}
window.mountParity = {
  effect,
  hostAssets: { basePath: Assets.resolver.basePath, warnings: assetWarnings },
  facade: {
    act: (action: unknown) => facade.act(action as never),
    view: () => facade.view(),
    snapshot: () => facade.snapshot(),
    send: (input: unknown) => facade.send(input as never),
    restore: (save: unknown) => facade.restore(save as never),
    bundleInfo: { index: facade.bundle.index, fingerprint: facade.program.fingerprint },
    // The session's current state feeds the program's read-only queries.
    queriedState: async () => {
      const current = await Effect.runPromise(SubscriptionRef.get(facade.session.current))
      return (await Effect.runPromise(facade.program.view(current.state))).state
    },
    snapshotState: async (snapshot: unknown) => {
      const state = await Effect.runPromise(facade.program.restore(snapshot as never))
      return (await Effect.runPromise(facade.program.view(state))).state
    },
    unmount: () => facade.unmount(),
    error: describe
  },
  closeEffect,
  resume: async (save: unknown) => {
    const resumedElement = document.createElement("div")
    resumedElement.id = "resumed"
    document.body.appendChild(resumedElement)
    const resumedScope = Scope.makeUnsafe("parallel")
    const resumed = await Effect.runPromise(Effect.provideService(mount(resumedElement, { ...options, snapshot: (save as { readonly snapshot: unknown }).snapshot as never }), Scope.Scope, resumedScope))
    const result = await Effect.runPromise(resumed.snapshot)
    await Effect.runPromise(resumed.shutdown)
    await Effect.runPromise(Scope.close(resumedScope, Exit.void))
    resumedElement.remove()
    return result
  }
}
`)
    const built = await Bun.build({
      entrypoints: [hostFile],
      target: "browser",
      minify: false,
      plugins: [{
        name: "workspace-aliases",
        setup(build) {
          build.onResolve({ filter: /^@lute-lang\/player$/ }, () => ({ path: `${repo}/packages/player/src/index.ts` }))
          build.onResolve({ filter: /^@lute-lang\/runtime$/ }, () => ({ path: `${repo}/packages/runtime/src/index.ts` }))
          build.onResolve({ filter: /^@lute-lang\/runtime\/browser$/ }, () => ({ path: `${repo}/packages/runtime/src/browser.ts` }))
        }
      }]
    })
    if (!built.success) throw new Error(built.logs.map((log) => log.message).join("\n"))
    mountScript = await built.outputs[0]!.text()
    rmSync(hostDir, { recursive: true, force: true })
    browser = await chromium.launch({ headless: true, args: ["--use-gl=swiftshader", "--enable-unsafe-swiftshader", "--autoplay-policy=no-user-gesture-required"] })
    mountPage = await browser.newPage({ viewport: { width: 1280, height: 720 } })
    await mountPage.route("**/mount-parity.js", (route) => route.fulfill({ status: 200, contentType: "text/javascript", body: mountScript }))
    page = await browser.newPage({ viewport: { width: 1280, height: 720 } })
    await page.goto(baseUrl, { waitUntil: "networkidle" })
    await page.waitForFunction(() => "luteDemo" in window)
    await mountPage.goto(baseUrl, { waitUntil: "networkidle" })
    await mountPage.evaluate((scriptUrl) => {
      document.body.innerHTML = `<div id="first"></div><div id="second"></div>`
      const script = document.createElement("script")
      script.type = "module"
      script.src = scriptUrl
      document.body.appendChild(script)
    }, `${baseUrl}mount-parity.js`)
    await mountPage.waitForFunction(() => "mountParity" in window)
  })

  test.afterAll(async () => {
    await browser?.close()
    server?.kill("SIGTERM")
  })

  const view = (): Promise<{ flow: string; countdownRemaining?: number }> => page.evaluate(() => (window as unknown as DemoWindow).luteDemo.view())
  const screenshot = async (name: string): Promise<void> => {
    const bytes = await page.screenshot({ path: `test-results/${name}.png` })
    const average = bytes.reduce((sum, byte) => sum + byte, 0) / Math.max(1, bytes.length)
    const variance = bytes.reduce((sum, byte) => sum + (byte - average) ** 2, 0) / Math.max(1, bytes.length)
    expect(variance).toBeGreaterThan(1)
  }

  type View = { flow: string; story: string; countdownRemaining?: number }
  const current = (): Promise<View> => page.evaluate(() => (window as unknown as DemoWindow).luteDemo.view()) as Promise<View>
  const freshGame = async (): Promise<void> => {
    await page.evaluate(() => localStorage.clear())
    await page.reload({ waitUntil: "networkidle" })
    await page.waitForFunction(() => "luteDemo" in window)
    await page.waitForFunction(async () => (await (window as unknown as DemoWindow).luteDemo.view()).flow === "title")
    // New game has the initial focus.
    await page.locator("#player").focus()
    await page.keyboard.press("Enter")
    await page.waitForFunction(async () => (await (window as unknown as DemoWindow).luteDemo.view()).flow === "playing")
  }
  /** One reader step: answer a choice with its first option, else advance. */
  const step = async (now: View): Promise<void> => {
    await page.keyboard.press(now.story === "choice" ? "1" : "Enter")
    await page.waitForTimeout(90)
  }

  test("starts on the title screen and New game starts playing", async () => {
    await page.waitForFunction(async () => (await (window as unknown as DemoWindow).luteDemo.view()).flow === "title")
    await screenshot("title")
    await freshGame()
    expect((await current()).flow).toBe("playing")
  })

  test("plays to an ending with real keyboard input; the timed choice expires", async () => {
    // A full playthrough under SwiftShader on a CI runner takes minutes.
    test.setTimeout(600_000)
    await freshGame()
    let timedExpired = false
    let shotMid = false
    const deadline = Date.now() + 540_000
    const trail: string[] = []
    for (let i = 0; Date.now() < deadline; i++) {
      const now = await current()
      const state = `${now.flow}/${now.story}`
      if (trail.at(-1)?.endsWith(state) !== true) trail.push(`${i}:${state}`)
      if (now.flow === "ended" || now.flow === "error" || now.story === "halted") break
      if (!shotMid && (now.story === "shown" || now.story === "typing") && i > 30) {
        await screenshot("mid-game")
        shotMid = true
      }
      if (now.story === "choice" && now.countdownRemaining !== undefined) {
        // Let the countdown run out: the runtime takes the branch default.
        await page.waitForFunction(async () => (await (window as unknown as DemoWindow).luteDemo.view()).story !== "choice", undefined, { timeout: 15_000 })
        timedExpired = true
        continue
      }
      await step(now)
    }
    expect((await current()).flow, trail.slice(-40).join(" ")).toBe("ended")
    expect(timedExpired).toBe(true)
    expect(shotMid).toBe(true)
    await screenshot("ending")
  })

  test("quick save then quick load returns to the saved choice point", async () => {
    await freshGame()
    for (let i = 0; i < 300 && (await current()).story !== "choice"; i++) await step(await current())
    expect((await current()).story).toBe("choice")
    await page.keyboard.press("F5")
    await page.waitForTimeout(300)
    await page.keyboard.press("1")
    for (let i = 0; i < 20; i++) await step(await current())
    await page.keyboard.press("F9")
    // Loading while playing asks for confirmation; Yes has the focus.
    await page.waitForTimeout(200)
    await page.keyboard.press("Enter")
    await page.waitForFunction(async () => (await (window as unknown as DemoWindow).luteDemo.view()).story === "choice", undefined, { timeout: 10_000 })
    await screenshot("after-load")
  })

  test("mount parity resumes snapshots, exposes the bundle, and mirrors facade Effects", async () => {
    let bundleRequests = 0
    mountPage.on("request", (request) => {
      if (request.url().endsWith("/bundle/project.index.json")) bundleRequests += 1
    })
    const result = await mountPage.evaluate(async () => {
      const api = (window as unknown as {
        mountParity: {
          effect: {
            act: (action: unknown) => Promise<void>
            view: () => Promise<{ readonly flow: string; readonly story: string }>
            snapshot: () => Promise<any>
            send: (input: unknown) => Promise<void>
            error: (error: unknown) => unknown
            bundleInfo: { readonly index: { readonly capabilitySnapshot: string }; readonly fingerprint: string }
          }
          hostAssets: { readonly basePath: string; readonly warnings: ReadonlyArray<string> }
          facade: {
            act: (action: unknown) => Promise<void>
            view: () => Promise<{ readonly flow: string; readonly story: string }>
            snapshot: () => Promise<any>
            send: (input: unknown) => Promise<void>
            restore: (save: unknown) => Promise<void>
            error: (error: unknown) => unknown
            bundleInfo: { readonly index: { readonly capabilitySnapshot: string }; readonly fingerprint: string }
            queriedState: () => Promise<unknown>
            snapshotState: (snapshot: unknown) => Promise<unknown>
            unmount: () => Promise<void>
          }
          closeEffect: () => Promise<void>
          resume: (save: unknown) => Promise<any>
        }
      }).mountParity
      const waitPlaying = async (view: () => Promise<{ readonly flow: string }>): Promise<void> => {
        for (let attempt = 0; attempt < 100; attempt += 1) {
          if ((await view()).flow === "playing") return
          await new Promise((resolve) => setTimeout(resolve, 50))
        }
        throw new Error("player did not reach playing")
      }
      await api.effect.act({ type: "newGame" })
      await waitPlaying(api.effect.view)
      const save = await api.effect.snapshot()
      const resumed = await api.resume(save)
      await api.facade.act({ type: "newGame" })
      await waitPlaying(api.facade.view)
      const facadeSave = await api.facade.snapshot()
      const facadeQueried = await api.facade.queriedState()
      const facadeSnapshotState = await api.facade.snapshotState(facadeSave.snapshot)
      const corrupt = JSON.parse(JSON.stringify(save)) as { snapshot: { project: string } }
      corrupt.snapshot.project = "wrong-project"
      const effectError = await api.effect.send({ type: "raiseOccasion", occasion: "start" }).catch(api.effect.error)
      const facadeError = await api.facade.send({ type: "raiseOccasion", occasion: "start" }).catch(api.facade.error)
      const facadeRestoreError = await api.facade.restore(corrupt).catch(api.facade.error)
      await api.facade.unmount()
      await api.closeEffect()
      return {
        saveState: JSON.stringify(save.snapshot.world.state),
        facadeQueried: JSON.stringify(facadeQueried),
        facadeSnapshotState: JSON.stringify(facadeSnapshotState),
        resumedState: JSON.stringify(resumed.snapshot.world.state),
        saveProject: save.snapshot.project,
        facadeProject: facadeSave.snapshot.project,
        effectFingerprint: api.effect.bundleInfo.fingerprint,
        facadeFingerprint: api.facade.bundleInfo.fingerprint,
        effectCapability: api.effect.bundleInfo.index.capabilitySnapshot,
        facadeCapability: api.facade.bundleInfo.index.capabilitySnapshot,
        hostBasePath: api.hostAssets.basePath,
        hostWarnings: api.hostAssets.warnings,
        effectError,
        facadeError,
        facadeRestoreError
      }
    })
    expect(result.resumedState).toBe(result.saveState)
    expect(result.effectFingerprint).toBe(result.saveProject)
    expect(result.facadeFingerprint).toBe(result.facadeProject)
    expect(result.effectCapability).toBe(result.facadeCapability)
    expect(result.facadeError).toEqual(result.effectError)
    expect(result.facadeRestoreError).toBeTruthy()
    expect(result.facadeQueried).toBe(result.facadeSnapshotState)
    expect(result.facadeQueried).not.toBe("undefined")
    expect(result.hostBasePath).toBe("forge/")
    expect(result.hostWarnings.some((warning: string) => warning.includes("already initialized"))).toBe(false)
    expect(bundleRequests).toBeGreaterThanOrEqual(1)
  })
  test("logs no console errors", () => {
    expect(errors).toEqual([])
  })
})
