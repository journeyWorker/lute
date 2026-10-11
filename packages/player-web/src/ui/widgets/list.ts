import { Container, Graphics, Text } from "pixi.js"
import type { Theme } from "../../engine/types.js"
import { Widget } from "./base.js"

/** Virtualized scroll model used by ScrollList and unit tests. */
export function visibleRange(total: number, rowHeight: number, viewportHeight: number, offset: number): { start: number; end: number } {
  const safeHeight = Math.max(rowHeight, 1)
  const start = Math.max(0, Math.floor(Math.max(0, offset) / safeHeight))
  const end = Math.min(total, Math.ceil((Math.max(0, offset) + Math.max(0, viewportHeight)) / safeHeight) + 1)
  return { start, end }
}

/** A virtualized, keyboard/pointer/wheel scroll list. */
export class ScrollList<T> extends Widget {
  private readonly viewport: Graphics
  private readonly content: Container
  private readonly scrollbar: Graphics
  private readonly rows: Map<number, Container> = new Map()
  private readonly values: ReadonlyArray<T>
  private readonly renderRow: (value: T, index: number) => Container
  private readonly rowHeight: number
  private readonly viewportHeight: number
  private offset = 0
  private dragging = false
  private dragStart = 0
  private dragOffset = 0

  constructor(values: ReadonlyArray<T>, theme: Theme, renderRow: (value: T, index: number) => Container, rowHeight = 52, viewportHeight = 360, width = 600) {
    super(theme, width, viewportHeight)
    this.values = values
    this.renderRow = renderRow
    this.rowHeight = rowHeight
    this.viewportHeight = viewportHeight
    this.viewport = new Graphics().rect(0, 0, width, viewportHeight).fill({ color: 0xffffff, alpha: 0.001 })
    this.viewport.eventMode = "static"
    this.content = new Container()
    this.scrollbar = new Graphics()
    this.addChild(this.content, this.viewport, this.scrollbar)
    this.viewport.on("wheel", (event) => { this.scrollBy(event.deltaY) })
    this.viewport.on("pointerdown", (event) => {
      this.dragging = true
      this.dragStart = event.global.y
      this.dragOffset = this.offset
    })
    this.viewport.on("pointermove", (event) => {
      if (this.dragging) this.setOffset(this.dragOffset + this.dragStart - event.global.y)
    })
    this.viewport.on("pointerup", () => { this.dragging = false })
    this.viewport.on("pointerupoutside", () => { this.dragging = false })
    this.refresh()
  }

  /** Current scroll offset in pixels. */
  get scrollOffset(): number {
    return this.offset
  }

  /** Maximum scroll offset in pixels. */
  get maxOffset(): number {
    return Math.max(0, this.values.length * this.rowHeight - this.viewportHeight)
  }

  /** Scroll by a pixel delta. */
  scrollBy(delta: number): void {
    this.setOffset(this.offset + delta)
  }

  /** Set absolute scroll offset and update only visible rows. */
  setOffset(offset: number): void {
    this.offset = Math.max(0, Math.min(this.maxOffset, offset))
    this.refresh()
  }

  /** Handle keyboard list navigation. */
  handleKey(key: string): boolean {
    if (key === "ArrowDown") this.scrollBy(this.rowHeight)
    else if (key === "ArrowUp") this.scrollBy(-this.rowHeight)
    else if (key === "PageDown") this.scrollBy(this.viewportHeight)
    else if (key === "PageUp") this.scrollBy(-this.viewportHeight)
    else if (key === "Home") this.setOffset(0)
    else if (key === "End") this.setOffset(this.maxOffset)
    else return false
    return true
  }

  activate(): void {}

  private refresh(): void {
    const range = visibleRange(this.values.length, this.rowHeight, this.viewportHeight, this.offset)
    for (const [index, row] of this.rows) {
      if (index < range.start || index >= range.end) {
        row.removeFromParent()
        this.rows.delete(index)
      }
    }
    for (let index = range.start; index < range.end; index += 1) {
      if (!this.rows.has(index)) {
        const value = this.values[index]
        if (value !== undefined) {
          const row = this.renderRow(value, index)
          row.position.y = index * this.rowHeight
          this.rows.set(index, row)
          this.content.addChild(row)
        }
      }
    }
    this.content.position.y = -this.offset
    const ratio = this.values.length === 0 ? 1 : this.viewportHeight / (this.values.length * this.rowHeight)
    const thumbHeight = Math.max(24, this.viewportHeight * Math.min(1, ratio))
    const thumbY = this.maxOffset === 0 ? 0 : (this.offset / this.maxOffset) * (this.viewportHeight - thumbHeight)
    this.scrollbar.clear().roundRect(this.width - 10, thumbY, 8, thumbHeight, 4).fill({ color: this.theme.accent, alpha: 0.8 })
  }
}

/** Rounded themed panel used as a screen background. */
export class Panel extends Graphics {
  readonly panelWidth: number
  readonly panelHeight: number
  private theme: Theme

  constructor(width: number, height: number, theme: Theme, radius = 14) {
    super()
    this.panelWidth = width
    this.panelHeight = height
    this.theme = theme
    this.draw(radius)
  }

  /** Repaint the panel with a new theme. */
  setTheme(theme: Theme): void {
    this.theme = theme
    this.draw(14)
  }

  private draw(radius: number): void {
    this.clear().roundRect(0, 0, this.panelWidth, this.panelHeight, radius).fill({ color: this.theme.panel, alpha: this.theme.panelAlpha })
  }
}

/** Themed text label. */
export class Label extends Text {
  constructor(text: string, theme: Theme, size = 22) {
    super({ text, style: { fontFamily: theme.font, fontSize: size, fill: theme.text } })
  }
}
