import { describe, expect, it } from "vitest"
import { mapTextStyle } from "../../src/text/richText.js"

describe("text style mapping", () => {
  it("merges named bindings in declaration order", () => {
    const style = mapTextStyle(["base", "emphasis"], {
      base: { color: 0xffffff, fontSize: 20, letterSpacing: 1 },
      emphasis: { fontStyle: "italic", color: 0xff0000 }
    })
    expect(style).toEqual({ color: 0xff0000, fontSize: 20, letterSpacing: 1, fontStyle: "italic" })
  })
})
