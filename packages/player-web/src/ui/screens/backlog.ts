import { Container, Text } from "pixi.js"
import type { BacklogEntry, PlayerAction, ScreenModel } from "@lute-lang/player"
import type { Theme } from "../../engine/types.js"
import { Button, Label, ScrollList } from "../widgets/index.js"
import { ScreenView, type EmitAction } from "./common.js"

/** Backlog screen with virtualized entries and voice replay actions. */
export class BacklogScreen extends ScreenView {
  private readonly list: ScrollList<BacklogEntry>
  constructor(model: Extract<ScreenModel, { _tag: "Backlog" }>, theme: Theme, emit: EmitAction) {
    super("Backlog", theme, emit, 1000, 660)
    this.list = new ScrollList(model.entries, theme, (entry) => this.row(entry, theme, emit), 72, 520, 900)
    this.list.position.set(36, 90)
    this.addChild(this.list)
    this.list.setOffset(this.list.maxOffset)
    this.button("Close", { type: "close" }, 36, 600)
  }
  override handleKey(key: string): boolean { return this.list.handleKey(key) || super.handleKey(key) }
  private row(entry: BacklogEntry, theme: Theme, emit: EmitAction): Container {
    const row = new Container()
    const name = new Label(entry.speakerName, theme, 18)
    const text = new Text({ text: entry.text, style: { fontFamily: theme.font, fontSize: 18, fill: theme.text, wordWrap: true, wordWrapWidth: entry.hasVoice ? 690 : 810 } })
    text.position.set(0, 25)
    row.addChild(name, text)
    if (entry.hasVoice) {
      const voice = new Button("Voice", theme, () => emit({ type: "replayVoice", lineId: entry.lineId }))
      voice.position.set(720, 14)
      row.addChild(voice)
    }
    return row
  }
}

/** In-game quick action bar. */
export class HudScreen extends Container {
  constructor(theme: Theme, emit: EmitAction, state: { readonly auto: boolean; readonly skip: boolean; readonly hidden?: boolean }) {
    super()
    const actions: ReadonlyArray<{ label: string; action: PlayerAction }> = [
      { label: state.auto ? "Auto: on" : "Auto", action: { type: "toggleAuto" } },
      { label: state.skip ? "Skip: on" : "Skip", action: { type: "toggleSkip" } },
      { label: "Log", action: { type: "openBacklog" } },
      { label: "Save", action: { type: "openSave" } },
      { label: "Load", action: { type: "openLoad" } },
      { label: "Q.Save", action: { type: "quickSave" } },
      { label: "Q.Load", action: { type: "quickLoad" } },
      { label: "Config", action: { type: "openSettings" } },
      { label: state.hidden ? "Show" : "Hide", action: { type: "hideUI" } }
    ]
    actions.forEach((item, index) => {
      const button = new Button(item.label, theme, () => emit(item.action))
      button.scale.set(0.55)
      button.position.set(index * 140, 0)
      this.addChild(button)
    })
  }
}
