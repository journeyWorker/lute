import { describe, expect, it } from "vitest"
import { buildRevealTimeline, graphemes } from "../../src/text/segments.js"

describe("reveal timeline", () => {
  it("segments emoji ZWJ and combining marks as one grapheme", () => {
    expect(graphemes("👩‍👩‍👧‍👦 é", "en")).toEqual(["👩‍👩‍👧‍👦", " ", "é"])
  })

  it("applies rates and preserves pauses", () => {
    const timeline = buildRevealTimeline([
      { _tag: "Text", text: "ab", styles: ["bold"] },
      { _tag: "Pause", seconds: 0.5 },
      { _tag: "Text", text: "界", styles: [], rate: 2 }
    ], 2, "ja")
    expect(timeline.graphemes.map((item) => item.start)).toEqual([0, 0.5, 1.5])
    expect(timeline.pauses).toEqual([{ segment: 1, start: 1, end: 1.5 }])
    expect(timeline.duration).toBe(1.75)
  })
  it("makes text immediate at zero speed", () => {
    const timeline = buildRevealTimeline([{ _tag: "Text", text: "漢字", styles: [] }], 0, "ja")
    expect(timeline.duration).toBe(0)
    expect(timeline.graphemes.every((item) => item.start === 0 && item.end === 0)).toBe(true)
  })
})
