import { describe, expect, it } from "vitest"
import { buildRevealTimeline, graphemes } from "../../src/text/segments.js"

describe("reveal timeline", () => {
  it("applies characters-per-second speed", () => {
    const timeline = buildRevealTimeline([{ _tag: "Text", text: "ab", styles: [] }], 2)
    expect(timeline.graphemes.map((item) => item.start)).toEqual([0, 0.5])
    expect(timeline.duration).toBe(1)
  })

  it("applies the per-segment rate multiplier", () => {
    const timeline = buildRevealTimeline([{ _tag: "Text", text: "ab", styles: [], rate: 2 }], 2)
    expect(timeline.graphemes[1]?.start).toBe(0.25)
    expect(timeline.duration).toBe(0.5)
  })

  it("keeps pauses as timeline gaps", () => {
    const timeline = buildRevealTimeline([
      { _tag: "Text", text: "ab", styles: ["bold"] },
      { _tag: "Pause", seconds: 0.5 },
      { _tag: "Text", text: "界", styles: [], rate: 2 }
    ], 2, "ja")
    expect(timeline.pauses).toEqual([{ segment: 1, start: 1, end: 1.5 }])
    expect(timeline.graphemes[2]?.start).toBe(1.5)
  })

  it("makes text immediate at zero speed", () => {
    const timeline = buildRevealTimeline([{ _tag: "Text", text: "漢字", styles: [] }], 0, "ja")
    expect(timeline.duration).toBe(0)
    expect(timeline.graphemes.every((item) => item.start === 0 && item.end === 0)).toBe(true)
  })

  it("segments CJK by grapheme cluster", () => {
    expect(graphemes("漢字かな", "ja")).toEqual(["漢", "字", "か", "な"])
  })

  it("keeps emoji ZWJ sequences intact", () => {
    expect(graphemes("👩‍👩‍👧‍👦", "en")).toEqual(["👩‍👩‍👧‍👦"])
  })

  it("keeps combining marks attached to their base", () => {
    expect(graphemes("é", "en")).toEqual(["é"])
  })
})
