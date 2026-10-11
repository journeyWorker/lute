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

describe("individual style binding properties", () => {
  it("maps fontWeight", () => expect(mapTextStyle(["s"], { s: { fontWeight: "700" } }).fontWeight).toBe("700"))
  it("maps fontStyle", () => expect(mapTextStyle(["s"], { s: { fontStyle: "italic" } }).fontStyle).toBe("italic"))
  it("maps color", () => expect(mapTextStyle(["s"], { s: { color: 0xff00ff } }).color).toBe(0xff00ff))
  it("maps alpha", () => expect(mapTextStyle(["s"], { s: { alpha: 0.5 } }).alpha).toBe(0.5))
  it("maps scale", () => expect(mapTextStyle(["s"], { s: { scale: 1.25 } }).scale).toBe(1.25))
  it("maps letterSpacing", () => expect(mapTextStyle(["s"], { s: { letterSpacing: 2 } }).letterSpacing).toBe(2))
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
