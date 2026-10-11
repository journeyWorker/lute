# Instructions

- Following Playwright test failed.
- Explain why, be concise, respect Playwright best practices.
- Provide a snippet of code with the fix, if possible.

# Test info

- Name: e2e/demo.e2e.ts >> player demo >> plays through the story with real keyboard input
- Location: test/e2e/demo.e2e.ts:73:2

# Error details

```
Test timeout of 120000ms exceeded.
```

```
Error: press: Target page, context or browser has been closed
```

# Test source

```ts
  1   | import { createServer } from "node:net"
  2   | import { spawn, type ChildProcessWithoutNullStreams } from "node:child_process"
  3   | import { existsSync } from "node:fs"
  4   | import { chromium, expect, test, type Browser, type Page } from "playwright/test"
  5   | 
  6   | interface DemoWindow extends Window {
  7   |   luteDemo: {
  8   |     view: () => Promise<{ flow: string; countdownRemaining?: number }>
  9   |     act: (action: unknown) => Promise<void>
  10  |   }
  11  | }
  12  | 
  13  | test.describe.serial("player demo", () => {
  14  |   let browser: Browser
  15  |   let page: Page
  16  |   let server: ChildProcessWithoutNullStreams
  17  |   let baseUrl = ""
  18  |   const errors: string[] = []
  19  | 
  20  |   test.beforeAll(async () => {
  21  |     const port = await new Promise<number>((resolvePort, reject) => {
  22  |       const socket = createServer()
  23  |       socket.once("error", reject)
  24  |       socket.listen(0, "127.0.0.1", () => {
  25  |         const address = socket.address()
  26  |         socket.close(() => resolvePort(typeof address === "object" && address !== null ? address.port : 0))
  27  |       })
  28  |     })
  29  |     const repo = process.cwd().replace(/\/packages\/player-web$/, "")
  30  |     const release = `${repo}/target/release/lute`
  31  |     const debug = `${repo}/target/debug/lute`
  32  |     const env = { ...process.env, PORT: String(port), ...(existsSync(release) ? { LUTE_BIN: release } : existsSync(debug) ? { LUTE_BIN: debug } : {}) }
  33  |     server = spawn("bun", ["scripts/demo.ts"], { cwd: `${repo}/packages/player-web`, env })
  34  |     await new Promise<void>((resolveServer, rejectServer) => {
  35  |       let output = ""
  36  |       server.stdout.on("data", (chunk: Buffer) => {
  37  |         output += chunk.toString()
  38  |         const match = output.match(/Lute player demo: (https?:\/\/[^\s]+)/)
  39  |         if (match !== null) {
  40  |           baseUrl = match[1]!
  41  |           resolveServer()
  42  |         }
  43  |       })
  44  |       server.stderr.on("data", (chunk: Buffer) => output += chunk.toString())
  45  |       server.once("exit", (code) => rejectServer(new Error(`demo exited ${code}: ${output}`)))
  46  |     })
  47  |     browser = await chromium.launch({ headless: true, args: ["--use-gl=swiftshader", "--enable-unsafe-swiftshader", "--autoplay-policy=no-user-gesture-required"] })
  48  |     page = await browser.newPage({ viewport: { width: 1280, height: 720 } })
  49  |     page.on("console", (message) => { if (message.type() === "error") errors.push(message.text()) })
  50  |     await page.goto(baseUrl, { waitUntil: "networkidle" })
  51  |   })
  52  | 
  53  |   test.afterAll(async () => {
  54  |     await browser?.close()
  55  |     server?.kill("SIGTERM")
  56  |   })
  57  | 
  58  |   const view = (): Promise<{ flow: string; countdownRemaining?: number }> => page.evaluate(() => (window as unknown as DemoWindow).luteDemo.view())
  59  |   const screenshot = async (name: string): Promise<void> => {
  60  |     const bytes = await page.screenshot({ path: `test-results/${name}.png` })
  61  |     const average = bytes.reduce((sum, byte) => sum + byte, 0) / Math.max(1, bytes.length)
  62  |     const variance = bytes.reduce((sum, byte) => sum + (byte - average) ** 2, 0) / Math.max(1, bytes.length)
  63  |     expect(variance).toBeGreaterThan(1)
  64  |   }
  65  | 
  66  |   test("starts on title screen", async () => {
  67  |     await page.waitForFunction(async () => (await (window as unknown as DemoWindow).luteDemo.view()).flow === "title")
  68  |     await screenshot("title")
  69  |     await page.mouse.click(100, 190)
  70  |     await page.waitForFunction(async () => (await (window as unknown as DemoWindow).luteDemo.view()).flow === "playing")
  71  |   })
  72  | 
  73  |   test("plays through the story with real keyboard input", async () => {
  74  |     await page.reload({ waitUntil: "networkidle" })
  75  |     await page.waitForFunction(async () => (await (window as unknown as DemoWindow).luteDemo.view()).flow === "title")
  76  |     await page.mouse.click(100, 190)
  77  |     await page.waitForFunction(async () => (await (window as unknown as DemoWindow).luteDemo.view()).flow === "playing")
  78  |     await page.locator("#player").focus()
  79  |     let saved = false
  80  |     let timedWaited = false
  81  |     const mid = true
  82  |     await screenshot("mid-chapter")
  83  |     await page.keyboard.press("F5")
  84  |     await page.waitForTimeout(150)
  85  |     const savedView = await view()
  86  |     await page.keyboard.press("Enter")
  87  |     await page.waitForTimeout(100)
  88  |     await page.keyboard.press("F9")
  89  |     await page.waitForTimeout(250)
  90  |     expect((await view()).flow).toBe(savedView.flow)
  91  |     saved = true
  92  |     for (let step = 0; step < 600; step += 1) {
  93  |       const current = await view()
  94  |       if (current.flow === "ended") break
  95  |       if (current.countdownRemaining !== undefined && current.countdownRemaining > 0 && !timedWaited) {
  96  |         timedWaited = true
  97  |         await page.waitForTimeout((current.countdownRemaining + 1) * 1000)
  98  |       } else {
> 99  |         await page.keyboard.press(current.countdownRemaining !== undefined ? "1" : "Enter")
      |                            ^ Error: press: Target page, context or browser has been closed
  100 |         await page.mouse.click(100, 180)
  101 |         await page.mouse.click(1100, 650)
  102 |       }
  103 |       await page.waitForTimeout(120)
  104 |     }
  105 |     expect(mid).toBe(true)
  106 |     expect(saved).toBe(true)
  107 |     expect(timedWaited).toBe(true)
  108 |     await page.waitForFunction(async () => (await (window as unknown as DemoWindow).luteDemo.view()).flow === "ended", undefined, { timeout: 30000 })
  109 |     await screenshot("ending")
  110 |     expect(errors).toEqual([])
  111 |   })
  112 | })
  113 | 
```