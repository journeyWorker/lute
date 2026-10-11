import { createServer } from "node:net"
import { spawn, type ChildProcessWithoutNullStreams } from "node:child_process"
import { existsSync } from "node:fs"
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
  let server: ChildProcessWithoutNullStreams
  let baseUrl = ""
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
    browser = await chromium.launch({ headless: true, args: ["--use-gl=swiftshader", "--enable-unsafe-swiftshader", "--autoplay-policy=no-user-gesture-required"] })
    page = await browser.newPage({ viewport: { width: 1280, height: 720 } })
    page.on("console", (message) => { if (message.type() === "error") errors.push(message.text()) })
    await page.goto(baseUrl, { waitUntil: "networkidle" })
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
    await page.waitForFunction(async () => (await (window as unknown as DemoWindow).luteDemo.view()).flow === "title")
    // New game has the initial focus.
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
    await freshGame()
    let timedExpired = false
    let shotMid = false
    for (let i = 0; i < 1200; i++) {
      const now = await current()
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
    expect((await current()).flow).toBe("ended")
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

  test("logs no console errors", () => {
    expect(errors).toEqual([])
  })
})
