import { isForbiddenLineEnd, isForbiddenLineStart, mapTextStyle } from "../../src/text/richText.js"
import { describe, expect, it } from "vitest"

describe("text style mapping", () => {
  it("merges named bindings in declaration order", () => {
    const style = mapTextStyle(["base", "emphasis"], {
      base: { color: 0xffffff, fontSize: 20, letterSpacing: 1 },
      emphasis: { fontStyle: "italic", color: 0xff0000 }
    })
    expect(style).toEqual({ color: 0xff0000, fontSize: 20, letterSpacing: 1, fontStyle: "italic" })
  })
})

describe("kinsoku line rules", () => {
  it("rejects every forbidden line-start punctuation", () => {
    for (const value of ["、", "。", "！", "？", "」", "』", "）", "】"]) {
      expect(isForbiddenLineStart(value)).toBe(true)
    }
  })

  it("rejects opening punctuation at line end", () => {
    for (const value of ["「", "『", "（", "【", "〈", "《"]) {
      expect(isForbiddenLineEnd(value)).toBe(true)
    }
  })
})
