import {
  CanvasTextMetrics,
  Container,
  Text,
  TextStyle,
  type TextStyleOptions
} from "pixi.js"
import type { Segment } from "@lute-lang/player"
import { graphemes } from "./segments.js"

/** Binding values accepted by a named text style. */
export interface TextStyleBinding {
  readonly fontWeight?: TextStyleOptions["fontWeight"]
  readonly fontStyle?: TextStyleOptions["fontStyle"]
  readonly color?: number | string
  readonly alpha?: number
  readonly scale?: number
  readonly letterSpacing?: number
  readonly fontFamily?: string
  readonly fontSize?: number
}

/** The plain map supplied by the bindings decoder. */
export type TextStyleBindings = Readonly<Record<string, TextStyleBinding>>

/** A style-bearing piece of text after line breaking. */
export interface StyledRun {
  readonly text: string
  readonly styles: ReadonlyArray<string>
  readonly line: number
}

const noStart = new Set(["、", "。", "！", "？", "」", "』", "）", "】", "〉", "》", "〕", "］", "｝"])
const noEnd = new Set(["「", "『", "（", "【", "〈", "《", "〔", "［", "｛"])
const cjk = /[\u2e80-\u2fff\u3000-\u30ff\u31a0-\u31bf\u3400-\u9fff\uf900-\ufaff]/u

/** Maps one named binding to Pixi's TextStyle options. */
export const mapTextStyle = (
  names: ReadonlyArray<string>,
  bindings: TextStyleBindings,
  base: TextStyleBinding = {}
): TextStyleBinding => {
  const result: TextStyleBinding = { ...base }
  for (const name of names) {
    const binding = bindings[name]
    if (binding !== undefined) Object.assign(result, binding)
  }
  return result
}

const optionsFor = (binding: TextStyleBinding, fallback: TextStyleBinding): TextStyleOptions => {
  const options: TextStyleOptions = {
    fontFamily: binding.fontFamily ?? fallback.fontFamily ?? "sans-serif",
    fontSize: binding.fontSize ?? fallback.fontSize ?? 24,
    fill: binding.color ?? fallback.color ?? 0xffffff,
    letterSpacing: binding.letterSpacing ?? fallback.letterSpacing ?? 0,
    wordWrap: false
  }
  if (binding.fontWeight !== undefined) options.fontWeight = binding.fontWeight
  if (binding.fontStyle !== undefined) options.fontStyle = binding.fontStyle
  return options
}

const widthOf = (text: string, binding: TextStyleBinding, fallback: TextStyleBinding): number => {
  if (text.length === 0) return 0
  const style = new TextStyle(optionsFor(binding, fallback))
  return CanvasTextMetrics.measureText(text, style).width
}

/**
 * Breaks styled segments into runs constrained to `width`. Latin text wraps at
 * whitespace; CJK text wraps at grapheme boundaries. Basic kinsoku rules keep
 * punctuation out of line starts and opening punctuation out of line ends.
 */
export const layoutStyledRuns = (
  segments: ReadonlyArray<Segment>,
  width: number,
  bindings: TextStyleBindings,
  fallback: TextStyleBinding = {},
  locale = ""
): ReadonlyArray<StyledRun> => {
  const result: StyledRun[] = []
  let line = 0
  let current = ""
  let currentStyles: ReadonlyArray<string> = []
  let currentBinding: TextStyleBinding = fallback

  const flush = (): void => {
    if (current.length > 0) result.push({ text: current, styles: currentStyles, line })
    current = ""
  }
  const sameStyle = (a: ReadonlyArray<string>, b: ReadonlyArray<string>): boolean =>
    a.length === b.length && a.every((value, index) => value === b[index])

  for (const segment of segments) {
    if (segment._tag === "Pause") continue
    const binding = mapTextStyle(segment.styles, bindings, fallback)
    const units = cjk.test(segment.text)
      ? graphemes(segment.text, locale)
      : segment.text.split(/(\s+)/u).filter((part: string) => part.length > 0)

    for (const unit of units) {
      const styleChanged = !sameStyle(currentStyles, segment.styles)
      const candidate = current + unit
      const over = current.length > 0 && widthOf(candidate, binding, fallback) > width
      if (styleChanged && current.length > 0) flush()
      if (over) {
        flush()
        line += 1
        currentStyles = segment.styles
        currentBinding = binding
      }
      if (current.length === 0) {
        currentStyles = segment.styles
        currentBinding = binding
      }
      if (noStart.has(unit) && current.length === 0 && result.length > 0) {
        const previous = result[result.length - 1]
        if (previous !== undefined && previous.line === line - 1 && previous.text.length > 0) {
          result[result.length - 1] = { ...previous, text: previous.text + unit }
          continue
        }
      }
      if (noEnd.has(unit) && widthOf(current + unit, currentBinding, fallback) >= width && current.length > 0) {
        flush()
        line += 1
        currentStyles = segment.styles
      }
      current += unit
    }
  }
  flush()
  return result
}

/** Creates Pixi Text children for laid-out runs and positions them in the box. */
export const renderStyledRuns = (
  target: Container,
  runs: ReadonlyArray<StyledRun>,
  bindings: TextStyleBindings,
  fallback: TextStyleBinding = {},
  lineHeight = 28
): ReadonlyArray<Text> => {
  const texts: Text[] = []
  const lineX = new Map<number, number>()
  for (const run of runs) {
    const binding = mapTextStyle(run.styles, bindings, fallback)
    const text = new Text({ text: run.text, style: new TextStyle(optionsFor(binding, fallback)) })
    const scale = binding.scale ?? 1
    text.scale.set(scale)
    text.alpha = binding.alpha ?? 1
    text.x = lineX.get(run.line) ?? 0
    text.y = run.line * lineHeight
    lineX.set(run.line, text.x + text.width)
    target.addChild(text)
    texts.push(text)
  }
  return texts
}

/** Returns true when a grapheme is CJK-like and should use character wrapping. */
export const isCjkGrapheme = (value: string): boolean => cjk.test(value)
