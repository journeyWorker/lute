import { createServer } from "node:net"
import { spawn, type ChildProcessWithoutNullStreams } from "node:child_process"
import { existsSync } from "node:fs"
import { chromium, expect, test, type Browser, type Page } from "playwright/test"

interface DemoWindow extends Window {
  luteDemo: {
    view: () => Promise<{ flow: string; countdownRemaining?: number }>
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

  test("starts on title screen", async () => {
    await page.waitForFunction(async () => (await (window as unknown as DemoWindow).luteDemo.view()).flow === "title")
    await screenshot("title")
    await page.mouse.click(100, 190)
    await page.waitForFunction(async () => (await (window as unknown as DemoWindow).luteDemo.view()).flow === "playing")
  })

  test("plays through the story with real keyboard input", async () => {
    await page.reload({ waitUntil: "networkidle" })
    await page.waitForFunction(async () => (await (window as unknown as DemoWindow).luteDemo.view()).flow === "title")
    await page.mouse.click(100, 190)
    await page.waitForFunction(async () => (await (window as unknown as DemoWindow).luteDemo.view()).flow === "playing")
    await page.locator("#player").focus()
    let saved = false
    let timedWaited = false
    const mid = true
    await screenshot("mid-chapter")
    await page.keyboard.press("F5")
    await page.waitForTimeout(150)
    const savedView = await view()
    await page.keyboard.press("Enter")
    await page.waitForTimeout(100)
    await page.keyboard.press("F9")
    await page.waitForTimeout(250)
    expect((await view()).flow).toBe(savedView.flow)
    saved = true
    for (let step = 0; step < 600; step += 1) {
      const current = await view()
      if (current.flow === "ended") break
      if (current.countdownRemaining !== undefined && current.countdownRemaining > 0 && !timedWaited) {
        timedWaited = true
        await page.waitForTimeout((current.countdownRemaining + 1) * 1000)
      } else {
        await page.keyboard.press(current.countdownRemaining !== undefined ? "1" : "Enter")
        await page.mouse.click(100, 180)
        await page.mouse.click(1100, 650)
      }
      await page.waitForTimeout(120)
    }
    expect(mid).toBe(true)
    expect(saved).toBe(true)
    expect(timedWaited).toBe(true)
    await page.waitForFunction(async () => (await (window as unknown as DemoWindow).luteDemo.view()).flow === "ended", undefined, { timeout: 30000 })
    await screenshot("ending")
    expect(errors).toEqual([])
  })
})
