import type { Segment } from "@lute-lang/player"

/** A grapheme in the reveal order of a line. Times are seconds from the line start. */
export interface RevealGrapheme {
  readonly grapheme: string
  readonly styles: ReadonlyArray<string>
  readonly segment: number
  readonly start: number
  readonly end: number
}

/** A pause in the reveal timeline. */
export interface RevealPause {
  readonly segment: number
  readonly start: number
  readonly end: number
}

/** The immutable, grapheme-aware typewriter timeline. */
export interface RevealTimeline {
  readonly graphemes: ReadonlyArray<RevealGrapheme>
  readonly pauses: ReadonlyArray<RevealPause>
  readonly duration: number
  readonly locale: string
}

const segmenter = (locale: string): Intl.Segmenter =>
  new Intl.Segmenter(locale || undefined, { granularity: "grapheme" })

/**
 * Builds the reveal schedule for a line. `rate` is a multiplier on the line's
 * characters-per-second setting; a zero speed makes every grapheme immediate.
 */
export const buildRevealTimeline = (
  segments: ReadonlyArray<Segment>,
  charactersPerSecond: number,
  locale = ""
): RevealTimeline => {
  const graphemes: RevealGrapheme[] = []
  const pauses: RevealPause[] = []
  let time = 0
  const speed = Math.max(0, charactersPerSecond)
  const iterator = segmenter(locale)

  segments.forEach((segment, segmentIndex) => {
    if (segment._tag === "Pause") {
      const seconds = Math.max(0, segment.seconds)
      pauses.push({ segment: segmentIndex, start: time, end: time + seconds })
      time += seconds
      return
    }

    const rate = segment.rate === undefined ? 1 : Math.max(0, segment.rate)
    const secondsPerGrapheme = speed === 0 || rate === 0 ? 0 : 1 / (speed * rate)
    for (const part of iterator.segment(segment.text)) {
      const start = time
      time += secondsPerGrapheme
      graphemes.push({
        grapheme: part.segment,
        styles: segment.styles,
        segment: segmentIndex,
        start,
        end: time
      })
    }
  })

  return { graphemes, pauses, duration: time, locale }
}

/** Alias kept descriptive for callers that treat the result as a reveal plan. */
export const segmentRevealTimeline = buildRevealTimeline

/** Returns all graphemes in a string using the same locale-aware segmentation as the timeline. */
export const graphemes = (text: string, locale = ""): ReadonlyArray<string> => {
  const iterator = segmenter(locale)
  return Array.from(iterator.segment(text), (part) => part.segment)
}
