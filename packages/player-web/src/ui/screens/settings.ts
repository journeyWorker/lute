import { Text } from "pixi.js"
import type { ScreenModel, Settings as PlayerSettings } from "@lute-lang/player"
import type { Theme } from "../../engine/types.js"
import { Label, Segmented, Slider, Toggle, Widget } from "../widgets/index.js"
import { ScreenView, type EmitAction } from "./common.js"

/** Settings editor with live values emitted on every control change. */
export class SettingsScreen extends ScreenView {
  private readonly settings: PlayerSettings
  private readonly sample: Text
  constructor(model: Extract<ScreenModel, { _tag: "Settings" }>, theme: Theme, emit: EmitAction) {
    super("Settings", theme, emit, 1100, 680)
    this.settings = model.settings
    this.sample = new Text({ text: "The quick brown fox jumps over the lazy dog.", style: { fontFamily: theme.font, fontSize: 20, fill: theme.text } })
    this.sample.position.set(40, 76)
    this.addChild(this.sample)
    this.row("Text speed", 125, new Slider(normalizeSpeed(model.settings.textSpeed), theme, (value) => { const textSpeed = value === 0 ? 0 : Math.round(10 + value * 190); emit({ type: "setSettings", settings: { textSpeed } }); this.sample.text = textSpeed === 0 ? "Instant text" : "The quick brown fox jumps over the lazy dog." }))
    this.row("Auto delay", 180, new Slider(normalizeDelay(model.settings.autoDelay), theme, (value) => emit({ type: "setSettings", settings: { autoDelay: Math.round(value * 20) } })))
    this.row("Skip mode", 235, new Segmented<"read" | "all">(["read", "all"], model.settings.skipMode, theme, (skipMode) => emit({ type: "setSettings", settings: { skipMode } })))
    this.row("Master volume", 290, this.volumeSlider(model.settings.volumes.master, theme, emit, "master"))
    this.row("Music volume", 345, this.volumeSlider(model.settings.volumes.music, theme, emit, "music"))
    this.row("SFX volume", 400, this.volumeSlider(model.settings.volumes.sfx, theme, emit, "sfx"))
    this.row("Voice volume", 455, this.volumeSlider(model.settings.volumes.voice, theme, emit, "voice"))
    this.row("Window opacity", 510, new Slider(model.settings.windowOpacity, theme, (windowOpacity) => emit({ type: "setSettings", settings: { windowOpacity } })))
    this.row("Read text colour", 565, new Toggle("Enabled", model.settings.readColour, theme, (readColour) => emit({ type: "setSettings", settings: { readColour } })))
    this.row("Locale", 620, new Segmented<string>(model.locales, model.settings.locale, theme, (locale) => emit({ type: "setSettings", settings: { locale } })))
    this.button("Close", { type: "close" }, 850, 620)
  }
  private row(name: string, y: number, control: Widget): void {
    const label = new Label(name, this.theme, 18)
    label.position.set(40, y + 7)
    control.position.set(250, y)
    this.addChild(label, control)
    this.focus.register(control)
  }
  private volumeSlider(value: number, theme: Theme, emit: EmitAction, channel: "master" | "music" | "sfx" | "voice"): Slider {
    return new Slider(value, theme, (next) => emit({ type: "setSettings", settings: { volumes: { ...this.settings.volumes, [channel]: next } } }))
  }
}
function normalizeSpeed(speed: number): number { if (speed <= 0) return 0; return Math.max(0, Math.min(1, (speed - 10) / 190)) }
function normalizeDelay(delay: number): number { return Math.max(0, Math.min(1, delay / 20)) }
