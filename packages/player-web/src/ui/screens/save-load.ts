import { Sprite, Text, Texture } from "pixi.js"
import type { PlayerAction, ScreenModel, SlotInfo } from "@lute-lang/player"
import type { Theme } from "../../engine/types.js"
import { Button, Panel } from "../widgets/index.js"
import { ScreenView, type EmitAction } from "./common.js"

/** Number of normal slots displayed per page. */
export const SLOTS_PER_PAGE = 6
/** Return the six slots for a page, including empty placeholders. */
export function pageSlots(slots: ReadonlyArray<SlotInfo>, page: number): ReadonlyArray<SlotInfo | undefined> {
  const offset = Math.max(0, page) * SLOTS_PER_PAGE
  return Array.from({ length: SLOTS_PER_PAGE }, (_, index) => slots[offset + index])
}
/** Save/load slot grid with thumbnails and paging. */
export class SaveLoadScreen extends ScreenView {
  constructor(model: Extract<ScreenModel, { _tag: "SaveLoad" }>, theme: Theme, emit: EmitAction) {
    super(model.mode === "save" ? "Save game" : "Load game", theme, emit, 1000, 650)
    pageSlots(model.slots, model.page).forEach((slot, index) => {
      const x = 36 + (index % 2) * 460
      const y = 90 + Math.floor(index / 2) * 130
      this.slot(slot, model.mode, theme, emit, x, y, model.page * SLOTS_PER_PAGE + index)
    })
    if (model.page > 0) this.button("Previous", { type: "choose", option: `page:${model.page - 1}` }, 36, 595)
    if ((model.page + 1) * SLOTS_PER_PAGE < model.slots.length) this.button("Next", { type: "choose", option: `page:${model.page + 1}` }, 210, 595)
    this.button("Close", { type: "close" }, 770, 595)
  }
  private slot(slot: SlotInfo | undefined, mode: "save" | "load", theme: Theme, emit: EmitAction, x: number, y: number, emptyIndex: number): void {
    const panel = new Panel(420, 130, theme, 8)
    panel.position.set(x, y)
    this.addChild(panel)
    if (slot?.thumbnail !== undefined) {
      const image = new Sprite(Texture.from(slot.thumbnail))
      image.width = 110
      image.height = 80
      image.position.set(x + 10, y + 10)
      this.addChild(image)
    }
    const label = new Text({ text: slot?.label ?? "Empty slot", style: { fontFamily: theme.font, fontSize: 18, fill: theme.text } })
    label.position.set(x + 135, y + 20)
    this.addChild(label)
    const date = new Text({ text: slot?.savedAt ?? "", style: { fontFamily: theme.font, fontSize: 14, fill: theme.text } })
    date.position.set(x + 135, y + 48)
    this.addChild(date)
    if (slot !== undefined) {
      const action: PlayerAction = mode === "save" ? { type: "saveSlot", slot: slot.slot } : { type: "loadSlot", slot: slot.slot }
      const button = new Button(mode === "save" ? "Save" : "Load", theme, () => emit(action))
      button.scale.set(0.6)
      button.position.set(x + 270, y + 72)
      this.addChild(button)
      this.focus.register(button)
      if (mode === "save") {
        const remove = new Button("Delete", theme, () => emit({ type: "deleteSlot", slot: slot.slot }))
        remove.scale.set(0.6)
        remove.position.set(x + 330, y + 72)
        this.addChild(remove)
        this.focus.register(remove)
      }
    } else if (mode === "save") {
      const button = new Button("Save", theme, () => emit({ type: "saveSlot", slot: emptyIndex }))
      button.scale.set(0.6)
      button.position.set(x + 270, y + 72)
      this.addChild(button)
      this.focus.register(button)
    }
  }
}
