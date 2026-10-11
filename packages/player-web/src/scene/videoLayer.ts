import { Effect } from "effect"
import { Container, Sprite, Texture } from "pixi.js"
import type { Bindings, Stage } from "@lute-lang/player"
import type { WorldShape, Theme } from "../engine/types.js"
import { assetFile, clearContainer, placeholder, type TextureAssets } from "./shared.js"

export class VideoLayer {
  readonly target: Container
  private readonly assets: TextureAssets
  private readonly bindings: Bindings
  private readonly design: { readonly width: number; readonly height: number }
  private readonly theme: Theme
  private readonly logger: (message: string) => void
  private active: HTMLVideoElement | undefined
  constructor(parent: Container, _world: WorldShape, bindings: Bindings, assets: TextureAssets, design: { readonly width: number; readonly height: number }, theme: Theme, logger: (message: string) => void = console.warn) {
    this.target = parent
    this.bindings = bindings
    this.assets = assets
    this.design = design
    this.theme = theme
    this.logger = logger
    parent.label = parent.label || "VideoLayer"
  }
  readonly show = (video: Stage["video"], blocking: boolean, skip = false): Effect.Effect<void> => {
    clearContainer(this.target)
    this.active = undefined
    if (video === null || video === undefined) return Effect.void
    const file = assetFile(this.bindings, video.assetId)
    const texture = file === undefined ? undefined : this.assets.video(file) ?? this.assets.texture(file)
    if (texture === undefined) {
      this.logger(`Missing binding video:${video.assetId}`)
      placeholder(this.target, `video:${video.assetId}`, this.theme, this.design.width, this.design.height)
      return Effect.void
    }
    const sprite = new Sprite(texture)
    sprite.width = this.design.width
    sprite.height = this.design.height
    this.target.addChild(sprite)
    const resource = texture.source.resource
    if (!(resource instanceof HTMLVideoElement)) return Effect.void
    this.active = resource
    resource.currentTime = 0
    resource.loop = false
    if (skip) {
      if (Number.isFinite(resource.duration) && resource.duration > 0) resource.currentTime = resource.duration
      resource.pause()
      return Effect.void
    }
    const play = resource.play()
    if (!blocking) return Effect.promise(() => play.then(() => undefined).catch(() => undefined))
    return Effect.callback<void>((resume) => {
      const finish = (): void => resume(Effect.succeed(undefined))
      resource.addEventListener("ended", finish, { once: true })
      resource.addEventListener("error", finish, { once: true })
      void play.catch(finish)
      return Effect.sync(() => {
        resource.removeEventListener("ended", finish)
        resource.removeEventListener("error", finish)
      })
    })
  }
  readonly hide = (): Effect.Effect<void> => Effect.sync(() => { if (this.active !== undefined) this.active.pause(); clearContainer(this.target); this.active = undefined })
}

export { VideoLayer as Sequence }
