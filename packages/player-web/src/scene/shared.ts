import { Container, Graphics, Sprite, Text, TextStyle, Texture } from "pixi.js"
import type { Bindings, Stage } from "@lute-lang/player"
import type { Theme } from "../engine/types.js"

export interface TextureAssets {
  readonly texture: (file: string) => Texture | undefined
  readonly video: (file: string) => Texture | undefined
}

export const assetFile = (bindings: Bindings, assetId: string | undefined): string | undefined =>
  assetId === undefined ? undefined : bindings.assets?.[assetId] ?? assetId

export const finite = (value: number | null | undefined, fallback: number): number =>
  value !== null && value !== undefined && Number.isFinite(value) ? value : fallback

export const textureFor = (assets: TextureAssets, file: string | undefined): Texture | undefined =>
  file === undefined ? undefined : assets.texture(file)

export const placeholder = (parent: Container, name: string, theme: Theme, width: number, height: number): Container => {
  const target = new Container()
  target.label = `Placeholder:${name}`
  const hash = Array.from(name).reduce((sum, char) => (sum * 33 + char.charCodeAt(0)) & 0xffffff, 0)
  const block = new Graphics().rect(0, 0, Math.max(1, width), Math.max(1, height)).fill({ color: hash || theme.accent, alpha: 0.8 })
  const text = new Text({ text: name, style: new TextStyle({ fontFamily: theme.font, fontSize: Math.max(12, Math.min(32, height / 5)), fill: theme.text }) })
  text.anchor?.set?.(0.5)
  text.x = width / 2
  text.y = height / 2
  target.addChild(block, text)
  parent.addChild(target)
  return target
}

export const spriteTexture = (bindings: Bindings, actor: Stage["actors"][number]): string | undefined => {
  const character = bindings.characters?.[actor.character]
  if (character === undefined) return undefined
  const sprite = actor.costume === undefined || actor.costume === null
    ? character.sprite
    : character.costumes?.[actor.costume] ?? character.sprite
  if (actor.emotion !== undefined && actor.emotion !== null) return sprite.emotions?.[actor.emotion] ?? sprite.default
  return sprite.default
}

export interface Size { readonly width: number; readonly height: number }
export interface Placement { readonly x: number; readonly y: number; readonly scale: number }

export const anchorPlacement = (
  anchor: { readonly x: number; readonly y?: number | null; readonly scale?: number | null } | undefined,
  design: Size,
  texture: Size
): Placement => {
  const scale = finite(anchor?.scale, 1)
  const x = finite(anchor?.x, 0.5) * design.width - texture.width * scale / 2
  const y = finite(anchor?.y, 1) * design.height - texture.height * scale
  return { x, y, scale }
}

export interface CgPlacement { readonly x: number; readonly y: number; readonly scale: number; readonly width: number; readonly height: number }
export const cgPlacement = (
  fit: "cover" | "contain",
  design: Size,
  texture: Size,
  scale = 1,
  x: number | null | undefined,
  y: number | null | undefined
): CgPlacement => {
  const ratio = fit === "cover"
    ? Math.max(design.width / Math.max(1, texture.width), design.height / Math.max(1, texture.height))
    : Math.min(design.width / Math.max(1, texture.width), design.height / Math.max(1, texture.height))
  const actualScale = ratio * Math.max(0, scale)
  const width = texture.width * actualScale
  const height = texture.height * actualScale
  return { x: x ?? (design.width - width) / 2, y: y ?? (design.height - height) / 2, scale: actualScale, width, height }
}

export const clearContainer = (container: Container): void => {
  for (const child of container.removeChildren()) child.destroy({ children: true })
}

export const transitionDuration = (duration: number | undefined, fallback = 0): number => Math.max(0, duration ?? fallback)
