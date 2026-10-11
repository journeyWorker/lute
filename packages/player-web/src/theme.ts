import type { Bindings } from "@lute-lang/player"
import type { Theme } from "./engine/types.js"

const DEFAULTS: Theme = {
  font: "sans-serif",
  text: 0xffffff,
  accent: 0xe0c080,
  panel: 0x101018,
  panelAlpha: 0.82,
  readText: 0x9a9aaa
}

/** Parse a CSS-style six digit colour, returning a supplied fallback on invalid input. */
export const parseColour = (value: unknown, fallback: number): number => {
  if (typeof value !== "string") return fallback
  const text = value.trim().replace(/^#/, "").replace(/^0x/i, "")
  return /^[0-9a-f]{6}$/i.test(text) ? Number.parseInt(text, 16) : fallback
}

/** Build the renderer theme from optional binding UI values. */
export const themeFromBindings = (bindings: Pick<Bindings, "ui"> | undefined): Theme => {
  const ui = bindings?.ui
  const colours = ui?.theme
  const alpha = typeof colours?.panelAlpha === "string" ? Number(colours.panelAlpha) : Number.NaN
  return {
    font: ui?.font ?? DEFAULTS.font,
    text: parseColour(colours?.text, DEFAULTS.text),
    accent: parseColour(colours?.accent, DEFAULTS.accent),
    panel: parseColour(colours?.panel, DEFAULTS.panel),
    panelAlpha: Number.isFinite(alpha) ? Math.max(0, Math.min(1, alpha)) : DEFAULTS.panelAlpha,
    readText: parseColour(colours?.readText, DEFAULTS.readText)
  }
}

export const defaultTheme: Theme = DEFAULTS
